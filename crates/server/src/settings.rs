//! The node's copy of the settings (ADM-002): loaded on startup, reloaded when any node changes a
//! setting, read by handlers without touching the database. The outgoing mail state (NTF-002,
//! NTF-004) is rebuilt with every reload: the transport and the switch follow the settings at
//! once.

use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use serde_json::Value;
use taskmaster_domain::Actor;
use taskmaster_domain::secret::Key;
use taskmaster_domain::settings::Snapshot;
pub use taskmaster_service::mail::{EnvMail, MailSource, Outgoing};
pub use taskmaster_service::settings::SetError;
use taskmaster_store::PgPool;
use taskmaster_store::bus::{self, BusEvent};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub struct SettingsCache {
    current: Arc<RwLock<Arc<Snapshot>>>,
    /// One reload at a time, so a reload that read older rows never replaces a newer snapshot.
    reloading: Arc<tokio::sync::Mutex<()>>,
    /// The environment's mail server, the transport's fallback (D-026).
    env_mail: Option<EnvMail>,
    /// The key secret settings are sealed with (NTF-004); `None` until one is set.
    secret_key: Option<Arc<Key>>,
    /// The mail state built from the snapshot and the environment.
    mail: Arc<RwLock<Arc<Outgoing>>>,
}

const RETRY_FIRST: Duration = Duration::from_millis(500);
const RETRY_MAX: Duration = Duration::from_secs(10);

impl SettingsCache {
    /// The current values; cheap, never waits for the database.
    pub fn get(&self) -> Arc<Snapshot> {
        Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// The environment's mail server, the transport's fallback (D-026).
    pub fn with_env_mail(mut self, env: Option<EnvMail>) -> Self {
        self.env_mail = env;
        self
    }

    /// The key secret settings are sealed with (`TASKMASTER_SECRET_KEY`, NTF-004).
    pub fn with_secret_key(mut self, key: Option<Key>) -> Self {
        self.secret_key = key.map(Arc::new);
        self
    }

    /// The key secret settings are sealed with, if one is set.
    pub fn secret_key(&self) -> Option<&Key> {
        self.secret_key.as_deref()
    }

    /// The environment's mail server, the transport's fallback (D-026).
    pub fn env_mail(&self) -> Option<&EnvMail> {
        self.env_mail.as_ref()
    }

    /// The outgoing email of this node: the transport, and whether the switch lets it send.
    pub fn mail(&self) -> Arc<Outgoing> {
        Arc::clone(&self.mail.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Reloads all values from the database, secrets included, and rebuilds the mail state.
    pub async fn reload(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        let _one_at_a_time = self.reloading.lock().await;
        let rows = taskmaster_store::settings::load(pool).await?;
        let (secrets, unreadable) =
            taskmaster_store::settings::load_secrets(pool, self.secret_key.as_deref()).await?;
        for key in unreadable {
            tracing::warn!(
                key,
                "a secret setting does not open under TASKMASTER_SECRET_KEY; its default applies"
            );
        }
        let (snapshot, errors) = Snapshot::from_stored(rows.into_iter().chain(secrets));
        for error in errors {
            tracing::warn!(%error, "ignoring a stored setting; its default applies");
        }
        let outgoing = taskmaster_service::mail::outgoing(&snapshot, self.env_mail.as_ref());
        if outgoing.enabled {
            tracing::info!(source = ?outgoing.source, "outgoing email is on");
        }
        *self.mail.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(outgoing);
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(snapshot);
        Ok(())
    }

    /// Changes a setting (`None` restores the default): validates it, stores it with an audit
    /// event and makes every node reload. Returns `false` if the value did not change.
    pub async fn set(
        &self,
        pool: &PgPool,
        key: &str,
        value: Option<&Value>,
        actor: Actor,
    ) -> Result<bool, SetError> {
        let changed =
            taskmaster_service::settings::set(pool, key, value, actor, self.secret_key.as_deref())
                .await?;
        if changed {
            // This node need not wait for its own notification.
            self.reload(pool).await?;
        }
        Ok(changed)
    }

    /// Changes an instance-wide flag (ADM-023): the same flow as a setting, and every node
    /// applies it without a restart.
    pub async fn set_flag(
        &self,
        pool: &PgPool,
        flag: &taskmaster_domain::settings::Flag,
        value: Option<bool>,
        actor: Actor,
    ) -> Result<bool, SetError> {
        let changed = taskmaster_service::settings::set_flag(pool, flag, value, actor).await?;
        if changed {
            self.reload(pool).await?;
        }
        Ok(changed)
    }

    /// Keeps this copy current while settings change on any node. A failed reload is retried
    /// with backoff until it succeeds.
    pub fn follow(
        &self,
        pool: PgPool,
        mut events: broadcast::Receiver<BusEvent>,
        stop: CancellationToken,
    ) -> JoinHandle<()> {
        let cache = self.clone();
        tokio::spawn(async move {
            // Set while a reload is owed: when to try again and the delay after that.
            let mut retry: Option<(Instant, Duration)> = None;
            loop {
                let retry_at = retry.map(|(at, _)| at);
                let event = tokio::select! {
                    () = stop.cancelled() => return,
                    event = events.recv() => Some(event),
                    () = sleep_until_or_forever(retry_at) => None,
                };
                let reload = match event {
                    None => true,
                    Some(Ok(BusEvent::Notified { channel, .. })) => channel == bus::SETTINGS,
                    Some(Ok(BusEvent::Resync) | Err(broadcast::error::RecvError::Lagged(_))) => {
                        true
                    }
                    Some(Err(broadcast::error::RecvError::Closed)) => return,
                };
                if !reload {
                    continue;
                }
                match cache.reload(&pool).await {
                    Ok(()) => retry = None,
                    Err(error) => {
                        let delay =
                            retry.map_or(RETRY_FIRST, |(_, delay)| (delay * 2).min(RETRY_MAX));
                        tracing::warn!(%error, retry_in = ?delay, "cannot reload settings; keeping the previous values");
                        retry = Some((Instant::now() + delay, delay));
                    }
                }
            }
        })
    }
}

async fn sleep_until_or_forever(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}
