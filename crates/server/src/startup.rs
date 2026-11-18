//! Bringing a node up once the database is reachable: migrations (ADM-048), settings, the
//! notification bus, the live gateway and the job runner. Readiness (ADM-033) turns green only at
//! the end.

use std::sync::atomic::Ordering;
use std::time::Duration;

use prostor_store::PgConnectOptions;
use prostor_store::bus::Bus;
use prostor_store::instance::SetupState;
use prostor_store::jobs::{Handlers, Runner, RunnerConfig};
use sqlx::migrate::MigrateError;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::AppState;
use crate::lifecycle::Phase;

const PING_TIMEOUT: Duration = Duration::from_secs(3);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("database migration failed: {0}")]
    Migration(String),
    #[error("database error during startup: {0}")]
    Database(#[from] sqlx::Error),
    #[error("cannot prepare the first-run setup: {0}")]
    Setup(#[from] prostor_service::Error),
    #[error("cannot use the files directory {dir}: {error}")]
    Files { dir: String, error: std::io::Error },
}

/// The background parts of a running node.
pub struct Services {
    runner: Runner,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl Services {
    /// Lets running jobs finish (up to their grace period), then stops listening.
    pub async fn stop(self) {
        self.runner.shutdown().await;
        self.stop.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

/// Waits for the database, applies pending migrations, starts the bus, loads settings and starts
/// the job runner, then marks the node ready. Returns `None` when `stop` fires while waiting for
/// the database or for another node's migrations. A failed migration is an error: the node must
/// not serve an unknown schema.
pub async fn start(
    state: &AppState,
    options: &PgConnectOptions,
    handlers: Handlers,
    runner: RunnerConfig,
    stop: &CancellationToken,
) -> Result<Option<Services>, StartupError> {
    if !wait_for_database(state, stop).await {
        return Ok(None);
    }

    state.lifecycle.set(Phase::Migrating);
    // Another node may hold the migration lock for a long time; a shutdown must not wait for it.
    // Dropping the migration closes its connection, which rolls back an unfinished migration.
    let migrated = tokio::select! {
        () = stop.cancelled() => return Ok(None),
        migrated = prostor_store::migrate(options) => migrated,
    };
    let report = migrated.map_err(|error| StartupError::Migration(explain(&error)))?;
    let version = report.latest.unwrap_or(0);
    if report.applied.is_empty() {
        tracing::info!(version, "database schema is up to date");
    } else {
        tracing::info!(applied = ?report.applied, version, "applied database migrations");
    }

    // Listen first, then load: a change committed in between still triggers a reload.
    let services_stop = CancellationToken::new();
    let (bus, bus_task) = Bus::start(state.db.clone(), services_stop.clone()).await?;
    let follow_task =
        state
            .settings
            .follow(state.db.clone(), bus.subscribe(), services_stop.clone());
    let access_task = state.access.follow(bus.subscribe(), services_stop.clone());
    let live_task = state
        .live
        .start(
            state.db.clone(),
            bus.subscribe(),
            state.access.clone(),
            state.settings.clone(),
            services_stop.clone(),
        )
        .await?;
    state.settings.reload(&state.db).await?;
    state.access.clear();
    announce_setup(state).await?;
    // Instances set up before fields existed get the default field set once (FLD-070).
    let language = state
        .settings
        .get()
        .get(&prostor_domain::settings::SYSTEM_LANGUAGE);
    if prostor_service::seed::ensure_default_fields(&state.db, language).await? {
        tracing::info!("created the default field set");
    }
    // And the built-in link types (LNK-002).
    if prostor_service::issue_links::ensure_default_types(&state.db, language).await? {
        tracing::info!("created the built-in link types");
    }
    // Texts rendered with older rules are rendered again in the background (D-031).
    if prostor_service::text::ensure_rendered(&state.db).await? {
        tracing::info!("queued rendering of stored texts with the current rules");
    }
    // After the setup state, which creates the instance row the key lives in.
    let names = prostor_service::people::Names::load(&state.db).await?;
    let _ = state.names.set(names);
    let signer = prostor_service::files::Signer::load(&state.db).await?;
    let _ = state.signer.set(signer);
    state
        .files
        .prepare()
        .await
        .map_err(|error| StartupError::Files {
            dir: state.files.root().display().to_string(),
            error,
        })?;
    let runner = Runner::start(state.db.clone(), handlers, runner, Some(bus.subscribe()));
    // The notification sweep fires due batches every few seconds, off the request path (plan
    // step 1.10 §3.4); the live purge's interval pattern. The mail state rides along so it can
    // queue the emails of what it fires (§5.2).
    let notify_task = tokio::spawn(notify_sweep(
        state.db.clone(),
        state.settings.clone(),
        services_stop.clone(),
    ));

    state.lifecycle.set(Phase::Ready);
    tracing::info!("Prostor is ready");
    Ok(Some(Services {
        runner,
        stop: services_stop,
        tasks: vec![bus_task, follow_task, access_task, live_task, notify_task],
    }))
}

/// Fires due notification batches until `stop`, logging and skipping errors: a failed step is
/// retried by the next one. Each step reads the mail state as it stands, so a settings change
/// applies on the next tick.
async fn notify_sweep(
    db: sqlx::PgPool,
    settings: crate::settings::SettingsCache,
    stop: CancellationToken,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(3));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            _ = tick.tick() => {}
        }
        let outgoing = settings.mail();
        if let Err(error) =
            prostor_service::notifications::sweep(&db, time::OffsetDateTime::now_utc(), &outgoing)
                .await
        {
            tracing::warn!(%error, "notification sweep failed");
        }
    }
}

/// While nobody has run the first-run setup, prints the code its wizard asks for (ADM-047).
async fn announce_setup(state: &AppState) -> Result<(), StartupError> {
    let configured = state.setup_code.as_deref();
    match prostor_service::setup::prepare(&state.db, configured).await? {
        SetupState::Complete => state.setup_done.store(true, Ordering::Release),
        SetupState::Pending { code } => {
            tracing::warn!(
                setup_code = %code,
                "Prostor is not set up yet: open it in a browser and enter the setup code {code}"
            );
        }
    }
    Ok(())
}

/// Retries until the database answers; `false` if `stop` fired first.
async fn wait_for_database(state: &AppState, stop: &CancellationToken) -> bool {
    let mut delay = Duration::from_millis(500);
    let mut waited = false;
    loop {
        let reachable = tokio::select! {
            () = stop.cancelled() => return false,
            reachable = prostor_store::ping(&state.db, PING_TIMEOUT) => reachable,
        };
        if reachable {
            if waited {
                tracing::info!("the database is reachable");
            }
            return true;
        }
        tracing::warn!(retry_in = ?delay, "the database is not reachable yet");
        waited = true;
        tokio::select! {
            () = stop.cancelled() => return false,
            () = tokio::time::sleep(delay) => {}
        }
        delay = (delay * 2).min(RETRY_MAX_DELAY);
    }
}

/// Migration errors in words an administrator can act on.
fn explain(error: &MigrateError) -> String {
    match error {
        MigrateError::VersionMissing(version) => format!(
            "the database has migration {version}, which this Prostor build does not know: a newer \
             version has run on it. Downgrades are not supported; run that version or restore a backup"
        ),
        MigrateError::VersionMismatch(version) => format!(
            "migration {version} applied to the database differs from the one in this build; \
             released migrations must never change"
        ),
        other => other.to_string(),
    }
}
