use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use trackflow_server::cli::{self, Command};
use trackflow_server::config::{Config, LogFormat, redact_password};
use trackflow_server::lifecycle::Phase;
use trackflow_server::web::EmbeddedAssets;
use trackflow_server::{AppState, VERSION, router, startup};
use trackflow_service::Passwords;
use trackflow_service::recovery::Target;
use trackflow_store::jobs::{Handlers, RunnerConfig};

/// How long shutdown waits for in-flight requests.
const REQUEST_DRAIN: Duration = Duration::from_secs(20);
/// How long shutdown waits for the database connections to close.
const POOL_CLOSE: Duration = Duration::from_secs(3);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = match cli::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(problem) => {
            let _ = write!(io::stderr(), "{problem}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    match command {
        Command::Serve => serve(Config::from_env()?).await,
        Command::RecoverAdmin(target) => recover_admin(Config::from_env()?, target).await,
        Command::Help => Ok(write!(io::stdout(), "{}", cli::USAGE)?),
        Command::Version => Ok(writeln!(io::stdout(), "trackflow {VERSION}")?),
    }
}

/// `trackflow recover-admin`: restores administrator access from the command line (ADM-051).
async fn recover_admin(config: Config, target: Target) -> anyhow::Result<()> {
    let options = trackflow_store::connect_options(&config.database.url)?;
    // The binary may be newer than the database; recovery needs the current schema.
    trackflow_store::migrate(&options)
        .await
        .context("cannot bring the database schema up to date")?;
    let pool = trackflow_store::pool(&options, 2);
    let letters = recovery_letters(&pool, &config).await;
    let recovered =
        trackflow_service::recovery::recover(&pool, &Passwords::new(1), target, &letters).await;
    pool.close().await;
    let recovered = match recovered {
        Ok(recovered) => recovered,
        Err(trackflow_service::Error::Invalid(problems)) => {
            let reasons: Vec<String> = problems
                .iter()
                .map(|problem| format!("{}: {}", problem.path, problem.message))
                .collect();
            anyhow::bail!("{}", reasons.join("; "));
        }
        Err(error) => anyhow::bail!("{error}"),
    };
    let user = &recovered.user;
    let mut out = io::stdout();
    writeln!(
        out,
        "{} system administrator {} ({}).",
        if recovered.created {
            "Created"
        } else {
            "Restored"
        },
        user.username,
        user.email.as_deref().unwrap_or("no email"),
    )?;
    writeln!(out, "One-time password: {}", recovered.password)?;
    if recovered.sessions_ended > 0 {
        writeln!(out, "Ended sessions: {}.", recovered.sessions_ended)?;
    }
    writeln!(
        out,
        "Sign in with it and change it at once in your account settings."
    )?;
    Ok(())
}

/// How the recovery report is written (ADM-051): the mail settings with the environment as the
/// fallback source (D-026) decide whether email is on, the stored address carries the link and
/// the instance language the words. A settings problem never blocks recovery: the defaults
/// leave email to the environment, secrets that do not open leave it off, as on the server.
async fn recovery_letters(
    pool: &trackflow_store::PgPool,
    config: &Config,
) -> trackflow_service::mail::Letters {
    let rows = trackflow_store::settings::load(pool)
        .await
        .unwrap_or_default();
    let secret_key = config
        .secret_key
        .as_ref()
        .map(|bytes| trackflow_domain::secret::Key::from(*bytes));
    let (secrets, _unreadable) = trackflow_store::settings::load_secrets(pool, secret_key.as_ref())
        .await
        .unwrap_or_default();
    let (snapshot, _errors) =
        trackflow_domain::settings::Snapshot::from_stored(rows.into_iter().chain(secrets));
    let env = config
        .mail
        .as_ref()
        .map(|mail| trackflow_service::mail::EnvMail {
            url: mail.smtp_url.clone(),
            from: mail.from.clone(),
        });
    let outgoing = trackflow_service::mail::outgoing(&snapshot, env.as_ref());
    trackflow_service::mail::Letters {
        enabled: outgoing.enabled,
        base_url: snapshot.get(&trackflow_domain::settings::BASE_URL),
        language: snapshot.get(&trackflow_domain::settings::SYSTEM_LANGUAGE),
    }
}

async fn serve(config: Config) -> anyhow::Result<()> {
    init_tracing(config.log_format);

    let options = trackflow_store::connect_options(&config.database.url)?;
    let pool = trackflow_store::pool(&options, config.database.max_connections);
    // The environment's mail server is the fallback of the admin mail settings (D-026, NTF-004).
    let env_mail = config
        .mail
        .as_ref()
        .map(|mail| trackflow_service::mail::EnvMail {
            url: mail.smtp_url.clone(),
            from: mail.from.clone(),
        });
    if env_mail.is_none() {
        tracing::info!(
            "no TRACKFLOW_SMTP_URL: the mail settings screen is the only way to turn email on"
        );
    }
    let secret_key = config.secret_key.map(|bytes| {
        tracing::info!("secret settings can be stored: TRACKFLOW_SECRET_KEY is set");
        trackflow_domain::secret::Key::from(bytes)
    });
    let gravatar = match &config.files.gravatar_url {
        Some(url) => Some(
            trackflow_service::pictures::Gravatar::new(url)
                .map_err(|error| anyhow::anyhow!("Gravatar import: {error}"))?,
        ),
        None => None,
    };
    let files = trackflow_files::FileStore::new(&config.files.dir);
    let state = AppState::new(Arc::new(EmbeddedAssets), pool)
        .with_trusted_proxies(config.trusted_proxies.clone())
        .with_setup_code(config.setup_code.clone())
        .with_secret_mail(env_mail.clone(), secret_key.clone())
        .with_files(files.clone(), config.files.upload_ceiling, gravatar);

    // Listen at once: liveness answers and readiness reports progress while the node starts.
    let listener = TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("cannot listen on {}", config.listen))?;
    tracing::info!(
        address = %listener.local_addr()?,
        version = VERSION,
        database = %redact_password(&config.database.url),
        "trackflow is listening"
    );

    let stop = CancellationToken::new();
    tokio::spawn({
        let stop = stop.clone();
        async move {
            shutdown_signal().await;
            stop.cancel();
        }
    });
    let mut server = tokio::spawn({
        // The peer address feeds sign-in throttling (crates/server/src/client.rs).
        let app = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        let serve =
            axum::serve(listener, app).with_graceful_shutdown(stop.clone().cancelled_owned());
        async move { serve.await }
    });

    let runner = RunnerConfig {
        workers: config.job_workers,
        ..RunnerConfig::default()
    };
    // Job kinds register here as features add them.
    let mut handlers = Handlers::new();
    let pool = state.db.clone();
    handlers.add::<trackflow_service::projects::ProjectDeletion, _, _>(move |_context, job| {
        let pool = pool.clone();
        async move { trackflow_service::projects::run_deletion(&pool, job).await }
    });
    let pool = state.db.clone();
    handlers.add::<trackflow_service::issues::jobs::FieldWork, _, _>(move |_context, work| {
        let pool = pool.clone();
        async move { trackflow_service::issues::jobs::run(&pool, work).await }
    });
    let pool = state.db.clone();
    handlers.add::<trackflow_service::text::RenderJob, _, _>(move |_context, job| {
        let pool = pool.clone();
        async move { trackflow_service::text::run_render(&pool, job).await }
    });
    handlers.add::<trackflow_service::files::FilesRemoval, _, _>({
        let files = files.clone();
        move |_context, job| {
            let files = files.clone();
            async move { trackflow_service::files::run_removal(&files, job).await }
        }
    });
    let pool = state.db.clone();
    handlers.add::<trackflow_service::uploads::UploadExpiry, _, _>(move |_context, job| {
        let pool = pool.clone();
        async move { trackflow_service::uploads::run_expiry(&pool, job).await }
    });
    let pool = state.db.clone();
    handlers.add::<trackflow_service::attachments::ImageInspection, _, _>({
        let files = files.clone();
        move |_context, job| {
            let pool = pool.clone();
            let files = files.clone();
            async move { trackflow_service::attachments::run_inspection(&pool, &files, job).await }
        }
    });
    // Always registered: the transport follows the settings, so it may appear or disappear
    // while the node runs. A job that finds none fails with the reason.
    {
        let settings = state.settings.clone();
        handlers.add::<trackflow_service::mail::Mail, _, _>(move |context, mail| {
            let settings = settings.clone();
            async move {
                let outgoing = settings.mail();
                let Some(mailer) = outgoing.transport.clone() else {
                    return Err(trackflow_store::jobs::JobError::Fail(
                        "outgoing email is not configured on this instance".to_owned(),
                    ));
                };
                if !outgoing.enabled {
                    return Err(trackflow_store::jobs::JobError::Fail(
                        "email is switched off on this instance".to_owned(),
                    ));
                }
                mailer.run(context, mail).await
            }
        });
    }
    let services = match startup::start(&state, &options, handlers, runner, &stop).await {
        Ok(services) => services,
        Err(error) => {
            tracing::error!(%error, "trackflow cannot start");
            stop.cancel();
            let _ = server.await;
            return Err(error.into());
        }
    };

    let served_early = tokio::select! {
        () = stop.cancelled() => None,
        served = &mut server => Some(served),
    };
    stop.cancel();
    state.lifecycle.set(Phase::Stopping);
    // Live sockets close at once, so their clients move to another node while requests drain.
    state.live.stop();

    // Jobs stop and requests drain at the same time, each with a limit, so the whole shutdown
    // fits the usual 30 s an orchestrator waits before killing the process.
    let jobs = async {
        if let Some(services) = services {
            services.stop().await;
        }
    };
    let requests = async {
        if let Some(served) = served_early {
            return served;
        }
        match tokio::time::timeout(REQUEST_DRAIN, &mut server).await {
            Ok(served) => served,
            Err(_) => {
                tracing::warn!(after = ?REQUEST_DRAIN, "requests still running; closing them");
                server.abort();
                Ok(Ok(()))
            }
        }
    };
    let ((), served) = tokio::join!(jobs, requests);
    if tokio::time::timeout(POOL_CLOSE, state.db.close())
        .await
        .is_err()
    {
        tracing::warn!("database connections did not close in time");
    }
    tracing::info!("trackflow stopped");
    served.context("the HTTP server task failed")??;
    Ok(())
}

fn init_tracing(format: LogFormat) {
    // PostgreSQL notices ("already exists, skipping") are noise at the default level.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx::postgres::notice=warn"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match format {
        LogFormat::Text => builder.init(),
        LogFormat::Json => builder.json().init(),
    }
}

/// Resolves on Ctrl+C or SIGTERM, so in-flight requests and jobs finish before exit.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for Ctrl+C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}
