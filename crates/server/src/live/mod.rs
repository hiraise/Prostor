//! The live gateway (ISS-048, ADM-002; D-040, architecture.md §5): one per node. Browsers keep a
//! WebSocket to `/api/v1/live`, subscribe to topics and get the events of the shared log that
//! they may see, replayed after a reconnect.
//!
//! Each connection has a reader and a writer task ([`connection`]); one pump task ([`pump`]) owns
//! all routing state and is the only sender of events, so replay and live events never interleave
//! out of order. Subscriptions are checked against access in [`topics`].

mod connection;
mod pump;
mod topics;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use taskmaster_store::PgPool;
use taskmaster_store::bus::BusEvent;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub use connection::upgrade;

use crate::access::AccessCache;
use crate::settings::SettingsCache;
use pump::{Command, Pump};

/// Outgoing messages a connection may have waiting; a full queue closes it with `TOO_SLOW`.
pub const QUEUE: usize = 512;
/// Most topics one connection holds; beyond that subscriptions are denied.
pub const TOPICS_MAX: usize = 500;
/// Most topics one `subscribe` may ask for; the rest are denied.
pub const SUBSCRIBE_MAX: usize = 100;
/// Largest message a client may send.
pub const MESSAGE_MAX: usize = 16 * 1024;
/// A connection that sends nothing (not even `ping`) this long is closed.
pub const IDLE: Duration = Duration::from_secs(60);
/// How far back the replay buffer reaches, in time and in events.
pub const BUFFER_AGE: Duration = Duration::from_secs(5 * 60);
pub const BUFFER_MAX: usize = 20_000;
/// How long the log keeps events, and how often older ones are removed.
pub const RETENTION: Duration = Duration::from_secs(60 * 60);
/// How long the inbox keeps its rows (plan step 1.10 §3.5: 180 days), removed on the same
/// schedule.
pub const INBOX_RETENTION: Duration = Duration::from_secs(180 * 24 * 60 * 60);
pub const PURGE_EVERY: Duration = Duration::from_secs(5 * 60);
/// How often the sessions and tokens of open connections are checked.
pub const SWEEP: Duration = Duration::from_secs(60);
/// People whose access is reloaded at once after an access change.
pub const ACCESS_RELOAD: usize = 8;
/// The pump looks for missed events this often, in case a notification was lost.
const TICK: Duration = Duration::from_secs(1);
/// Events read from the log per query.
const READ_BATCH: i64 = 1000;
/// Commands waiting for the pump.
const COMMANDS: usize = 4096;

/// What tests may make smaller or faster.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Events kept for replay at most.
    pub buffer_max: usize,
    /// Outgoing messages a connection may have waiting.
    pub queue: usize,
    /// How often sessions and tokens are checked.
    pub sweep: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            buffer_max: BUFFER_MAX,
            queue: QUEUE,
            sweep: SWEEP,
        }
    }
}

/// The node's gateway: a handle to the pump, set when the node starts.
#[derive(Clone)]
pub struct Gateway {
    inner: Arc<Inner>,
}

struct Inner {
    commands: OnceLock<mpsc::Sender<Command>>,
    /// Fires when the node turns stopping: every socket closes with `RESTART`.
    closing: CancellationToken,
    next_connection: AtomicU64,
    config: Config,
}

impl Default for Gateway {
    fn default() -> Self {
        Self::with_config(Config::default())
    }
}

impl Gateway {
    /// A gateway with other limits (tests use small and fast ones).
    pub fn with_config(config: Config) -> Self {
        Self {
            inner: Arc::new(Inner {
                commands: OnceLock::new(),
                closing: CancellationToken::new(),
                next_connection: AtomicU64::new(1),
                config,
            }),
        }
    }

    /// Starts the pump: reads the log's head and the last minutes into the replay buffer, then
    /// follows the bus until `stop` fires or the node turns stopping.
    pub async fn start(
        &self,
        db: PgPool,
        bus: broadcast::Receiver<BusEvent>,
        access: AccessCache,
        settings: SettingsCache,
        stop: CancellationToken,
    ) -> Result<JoinHandle<()>, sqlx::Error> {
        let (sender, commands) = mpsc::channel(COMMANDS);
        let pump = Pump::load(db, access, settings, self.inner.config).await?;
        let stop = {
            let both = CancellationToken::new();
            let closing = self.inner.closing.clone();
            let child = both.clone();
            tokio::spawn(async move {
                tokio::select! {
                    () = stop.cancelled() => {}
                    () = closing.cancelled() => {}
                }
                child.cancel();
            });
            both
        };
        let task = tokio::spawn(pump.run(commands, sender.downgrade(), bus, stop));
        // A node starts once; a second start keeps the first pump.
        let _ = self.inner.commands.set(sender);
        Ok(task)
    }

    /// The node turns stopping: closes every socket with `RESTART` and ends the pump.
    pub fn stop(&self) {
        self.inner.closing.cancel();
    }

    fn commands(&self) -> Option<&mpsc::Sender<Command>> {
        self.inner.commands.get()
    }

    fn config(&self) -> &Config {
        &self.inner.config
    }

    fn closing(&self) -> &CancellationToken {
        &self.inner.closing
    }

    fn next_connection(&self) -> u64 {
        self.inner.next_connection.fetch_add(1, Ordering::Relaxed)
    }
}

/// How a connection ends: set once by whoever ends it first, read by its writer.
#[derive(Default)]
struct Ending {
    token: CancellationToken,
    code: OnceLock<u16>,
}

impl Ending {
    /// Ends the connection with a close code.
    fn close(&self, code: u16) {
        let _ = self.code.set(code);
        self.token.cancel();
    }

    /// Ends it without a code of ours (the peer left or closed).
    fn quit(&self) {
        self.token.cancel();
    }
}
