//! Notifications between nodes over PostgreSQL LISTEN/NOTIFY (architecture.md §1, §3).
//!
//! A writer calls [`notify`] inside its transaction; PostgreSQL delivers the notification to every
//! listening node when the transaction commits, and drops it on rollback. Each node keeps one
//! listening connection and hands notifications to in-process subscribers. Notifications sent while
//! that connection is down are lost, so after reconnecting subscribers get [`BusEvent::Resync`]
//! and reload whatever they cache. The listening connection takes one slot of the pool.

use std::time::Duration;

use sqlx::postgres::PgListener;
use sqlx::{PgConnection, PgPool};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// A setting changed; the payload is its key.
pub const SETTINGS: &str = "taskmaster_settings";
/// A job was queued.
pub const JOBS: &str = "taskmaster_jobs";
/// Something that decides people's access changed: roles, assignments, groups, memberships,
/// capabilities, accounts (ACC-062). Nodes drop their compiled access.
pub const ACCESS: &str = "taskmaster_access";
/// Live events were published (D-040); the payload is the newest event ID.
pub const LIVE: &str = "taskmaster_live";
/// Every channel a node listens to.
const CHANNELS: [&str; 4] = [SETTINGS, JOBS, ACCESS, LIVE];

/// Subscribers that fall this far behind get `Lagged` and should resync.
const CAPACITY: usize = 1024;
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusEvent {
    Notified {
        channel: String,
        payload: String,
    },
    /// Notifications may have been missed: reload cached state.
    Resync,
}

/// Queues a notification; it is delivered when the surrounding transaction commits.
pub async fn notify(
    conn: &mut PgConnection,
    channel: &str,
    payload: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("select from pg_notify($1, $2)", channel, payload)
        .execute(conn)
        .await?;
    Ok(())
}

/// The node's end of the bus.
#[derive(Clone)]
pub struct Bus {
    sender: broadcast::Sender<BusEvent>,
}

impl Bus {
    /// Starts listening; forwards notifications to subscribers until `stop` fires.
    pub async fn start(
        pool: PgPool,
        stop: CancellationToken,
    ) -> Result<(Self, JoinHandle<()>), sqlx::Error> {
        let listener = listen(&pool).await?;
        let (sender, _) = broadcast::channel(CAPACITY);
        let task = tokio::spawn(forward(pool, listener, sender.clone(), stop));
        Ok((Self { sender }, task))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.sender.subscribe()
    }
}

async fn listen(pool: &PgPool) -> Result<PgListener, sqlx::Error> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener.listen_all(CHANNELS).await?;
    Ok(listener)
}

async fn forward(
    pool: PgPool,
    mut listener: PgListener,
    sender: broadcast::Sender<BusEvent>,
    stop: CancellationToken,
) {
    loop {
        let received = tokio::select! {
            () = stop.cancelled() => return,
            received = listener.try_recv() => received,
        };
        match received {
            Ok(Some(notification)) => {
                // No subscribers is fine: nobody caches anything yet.
                let _ = sender.send(BusEvent::Notified {
                    channel: notification.channel().to_owned(),
                    payload: notification.payload().to_owned(),
                });
            }
            // The connection dropped and the listener has already reconnected and listens again.
            Ok(None) => {
                tracing::info!("notification connection restored");
                let _ = sender.send(BusEvent::Resync);
            }
            Err(error) => {
                tracing::warn!(%error, "lost the notification connection, reconnecting");
                let Some(fresh) = reconnect(&pool, &stop).await else {
                    return;
                };
                listener = fresh;
                tracing::info!("notification connection restored");
                let _ = sender.send(BusEvent::Resync);
            }
        }
    }
}

async fn reconnect(pool: &PgPool, stop: &CancellationToken) -> Option<PgListener> {
    let mut delay = Duration::from_millis(250);
    loop {
        match listen(pool).await {
            Ok(listener) => return Some(listener),
            Err(error) => tracing::warn!(%error, "cannot reconnect the notification listener"),
        }
        tokio::select! {
            () = stop.cancelled() => return None,
            () = tokio::time::sleep(delay) => {}
        }
        delay = (delay * 2).min(RECONNECT_MAX_DELAY);
    }
}
