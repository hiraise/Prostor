//! The job runner: a fixed number of workers per node take jobs from the queue and run their
//! handlers.
//!
//! - Workers are woken by a notification when a job is queued and also poll, which covers delayed
//!   jobs, retries and missed notifications.
//! - While a handler runs, its lease is renewed; a job whose lease runs out (crashed node, lost
//!   database) goes back to the queue, so handlers must be idempotent.
//! - On shutdown the runner stops claiming, asks running handlers to stop and waits for them up to
//!   a grace period; whatever is still running then goes back to the queue without losing an attempt.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, broadcast};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::queue::{self, Claimed, JobKind, Outcome};
use crate::bus::{self, BusEvent};

/// Why a handler did not finish its job.
#[derive(Debug, Error)]
pub enum JobError {
    /// A temporary problem (database, network): the job runs again later with backoff.
    #[error("{0}")]
    Retry(String),
    /// The job cannot succeed: it fails without further attempts.
    #[error("{0}")]
    Fail(String),
    /// The job noticed it should stop ([`JobContext::check`]).
    #[error("the job was asked to stop")]
    Stopped,
}

impl From<sqlx::Error> for JobError {
    fn from(error: sqlx::Error) -> Self {
        JobError::Retry(error.to_string())
    }
}

pub type JobFuture = Pin<Box<dyn Future<Output = Result<(), JobError>> + Send>>;
type Handler = Arc<dyn Fn(JobContext, Value) -> JobFuture + Send + Sync>;

/// Handlers by job kind. A node claims only the kinds it has handlers for.
#[derive(Clone, Default)]
pub struct Handlers {
    by_kind: HashMap<&'static str, Handler>,
}

impl Handlers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the handler for jobs of kind `J`.
    pub fn add<J, F, Fut>(&mut self, handler: F) -> &mut Self
    where
        J: JobKind,
        F: Fn(JobContext, J) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), JobError>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        let erased: Handler = Arc::new(move |ctx, payload| {
            let handler = Arc::clone(&handler);
            Box::pin(async move {
                let job: J = serde_json::from_value(payload)
                    .map_err(|error| JobError::Fail(format!("invalid payload: {error}")))?;
                handler(ctx, job).await
            })
        });
        self.by_kind.insert(J::KIND, erased);
        self
    }

    fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self.by_kind.keys().map(|kind| (*kind).to_owned()).collect();
        kinds.sort();
        kinds
    }
}

/// What a running handler gets: the job's identity, the pool and a way to report progress and
/// to learn that it should stop.
#[derive(Clone)]
pub struct JobContext {
    inner: Arc<ContextInner>,
}

struct ContextInner {
    id: i64,
    attempt: i32,
    /// The claim this run holds; guards its writes to the job.
    token: i64,
    pool: PgPool,
    /// Fires on cancellation by a user, on shutdown and when the claim is lost.
    stop: CancellationToken,
    cancel_requested: AtomicBool,
    claim_lost: AtomicBool,
}

impl JobContext {
    fn new(id: i64, attempt: i32, token: i64, pool: PgPool, stop: CancellationToken) -> Self {
        Self {
            inner: Arc::new(ContextInner {
                id,
                attempt,
                token,
                pool,
                stop,
                cancel_requested: AtomicBool::new(false),
                claim_lost: AtomicBool::new(false),
            }),
        }
    }

    pub fn id(&self) -> i64 {
        self.inner.id
    }

    /// 1 for the first run, 2 for the first retry and so on.
    pub fn attempt(&self) -> i32 {
        self.inner.attempt
    }

    pub fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    /// `Err(Stopped)` when the job should stop; call between units of work.
    pub fn check(&self) -> Result<(), JobError> {
        if self.inner.stop.is_cancelled() {
            Err(JobError::Stopped)
        } else {
            Ok(())
        }
    }

    /// Resolves when the job should stop; for `select!` around long waits.
    pub async fn stopped(&self) {
        self.inner.stop.cancelled().await;
    }

    /// Records progress (`done` of `total` units, optional status line) and checks whether the
    /// job should stop. Call once per batch, not per item: every call is a database write.
    pub async fn progress(
        &self,
        done: i64,
        total: Option<i64>,
        status: Option<&str>,
    ) -> Result<(), JobError> {
        self.check()?;
        let answer = queue::set_progress(
            &self.inner.pool,
            self.inner.id,
            self.inner.token,
            done,
            total,
            status,
        )
        .await?;
        self.apply(answer);
        self.check()
    }

    /// Replaces the stored payload with `{}` once the handler no longer needs it, so secrets in it
    /// (the one-time links of account emails) do not stay in the database. A later run of the
    /// same job gets the empty payload and must treat it as done.
    pub async fn forget_payload(&self) -> Result<(), JobError> {
        queue::clear_payload(&self.inner.pool, self.inner.id, self.inner.token).await?;
        Ok(())
    }

    /// Applies the answer of a lease renewal or progress write.
    fn apply(&self, answer: Option<bool>) {
        match answer {
            Some(false) => {}
            Some(true) => {
                self.inner.cancel_requested.store(true, Ordering::SeqCst);
                self.inner.stop.cancel();
            }
            None => {
                self.inner.claim_lost.store(true, Ordering::SeqCst);
                self.inner.stop.cancel();
            }
        }
    }
}

/// Runner settings. The defaults suit production; tests shorten the intervals.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Jobs that run at the same time on this node.
    pub workers: usize,
    /// How long a claim holds without renewal; renewed every third of it.
    pub lease: Duration,
    /// How often idle workers look for due jobs without a notification.
    pub poll_interval: Duration,
    /// How often expired leases are returned to the queue.
    pub reap_interval: Duration,
    /// Delay before the first retry; doubles with every attempt, at most an hour.
    pub retry_base: Duration,
    /// How long shutdown waits for running handlers.
    pub shutdown_grace: Duration,
    /// Name of this node's workers in the `jobs.worker` column.
    pub worker_name: String,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            workers: 4,
            lease: Duration::from_secs(30),
            poll_interval: Duration::from_secs(5),
            reap_interval: Duration::from_secs(15),
            retry_base: Duration::from_secs(10),
            shutdown_grace: Duration::from_secs(20),
            worker_name: format!("{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()),
        }
    }
}

const MAX_RETRY_DELAY: Duration = Duration::from_secs(3600);

/// A started runner; [`Runner::shutdown`] stops it.
pub struct Runner {
    stop: CancellationToken,
    task: JoinHandle<()>,
}

struct Shared {
    pool: PgPool,
    handlers: Handlers,
    kinds: Vec<String>,
    config: RunnerConfig,
    wake: Notify,
    stop: CancellationToken,
    /// Claim tokens being run, by job ID, so shutdown can release those it had to abort.
    in_flight: Mutex<HashMap<i64, i64>>,
}

impl Runner {
    /// Starts the workers. `events` wakes them when a job is queued on any node.
    pub fn start(
        pool: PgPool,
        handlers: Handlers,
        config: RunnerConfig,
        events: Option<broadcast::Receiver<BusEvent>>,
    ) -> Self {
        let stop = CancellationToken::new();
        let shared = Arc::new(Shared {
            pool,
            kinds: handlers.kinds(),
            handlers,
            config,
            wake: Notify::new(),
            stop: stop.clone(),
            in_flight: Mutex::new(HashMap::new()),
        });
        if let Some(events) = events {
            tokio::spawn(wake_on_events(Arc::clone(&shared), events));
        }
        tokio::spawn(reap_expired(Arc::clone(&shared)));
        let task = tokio::spawn(dispatch(shared));
        Self { stop, task }
    }

    /// Stops claiming, asks running jobs to stop, waits for them up to the grace period and
    /// returns the unfinished ones to the queue.
    pub async fn shutdown(self) {
        self.stop.cancel();
        if let Err(error) = self.task.await {
            tracing::error!(%error, "job runner failed");
        }
    }
}

async fn dispatch(shared: Arc<Shared>) {
    let permits = Arc::new(Semaphore::new(shared.config.workers.max(1)));
    let mut running = JoinSet::new();
    tracing::info!(
        workers = shared.config.workers,
        kinds = ?shared.kinds,
        worker = %shared.config.worker_name,
        "job runner started"
    );

    loop {
        while running.try_join_next().is_some() {}
        let permit = tokio::select! {
            biased;
            () = shared.stop.cancelled() => break,
            permit = Arc::clone(&permits).acquire_owned() => match permit {
                Ok(permit) => permit,
                Err(_) => break,
            },
        };
        let claimed = if shared.kinds.is_empty() {
            Ok(None)
        } else {
            queue::claim(
                &shared.pool,
                &shared.kinds,
                &shared.config.worker_name,
                shared.config.lease,
            )
            .await
        };
        match claimed {
            Ok(Some(job)) => {
                // Registered before the task starts, so shutdown can release it even if the task
                // is aborted before its first poll.
                shared
                    .in_flight
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(job.id, job.token);
                running.spawn(execute(Arc::clone(&shared), job, permit));
            }
            Ok(None) => {
                drop(permit);
                tokio::select! {
                    () = shared.stop.cancelled() => break,
                    () = shared.wake.notified() => {}
                    () = tokio::time::sleep(shared.config.poll_interval) => {}
                }
            }
            Err(error) => {
                drop(permit);
                tracing::warn!(%error, "cannot take a job from the queue");
                tokio::select! {
                    () = shared.stop.cancelled() => break,
                    () = tokio::time::sleep(shared.config.poll_interval) => {}
                }
            }
        }
    }

    // Running handlers see the stop through their context (its token is a child of `stop`).
    let drained = tokio::time::timeout(shared.config.shutdown_grace, async {
        while running.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        running.abort_all();
        while running.join_next().await.is_some() {}
        let left: Vec<(i64, i64)> = shared
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .collect();
        for (id, token) in left {
            tracing::warn!(
                job = id,
                "job did not stop in time, returning it to the queue"
            );
            if let Err(error) = queue::complete(&shared.pool, id, token, Outcome::Released).await {
                tracing::error!(job = id, %error, "cannot return the job to the queue");
            }
        }
    }
    tracing::info!("job runner stopped");
}

async fn execute(shared: Arc<Shared>, job: Claimed, _permit: OwnedSemaphorePermit) {
    let Claimed {
        id,
        kind,
        payload,
        attempt,
        token,
        initiator_id,
    } = job;
    let span = tracing::info_span!("job", id, kind = %kind, attempt);
    let stop = shared.stop.child_token();
    let ctx = JobContext::new(id, attempt, token, shared.pool.clone(), stop.clone());

    let outcome = match shared.handlers.by_kind.get(kind.as_str()) {
        // Claims are limited to known kinds, so this only guards against a bug.
        None => Some(Outcome::Failed(format!("no handler for job kind {kind:?}"))),
        Some(handler) => {
            let _heartbeat = AbortOnDrop(tokio::spawn(renew_lease(
                shared.pool.clone(),
                ctx.clone(),
                shared.config.lease,
            )));
            // A separate task, so a panicking handler fails its job instead of the worker.
            let mut run = AbortOnDrop(tokio::spawn(
                handler(ctx.clone(), payload).instrument(span.clone()),
            ));
            let result = (&mut run.0).await;
            interpret(&ctx, result, attempt, &shared.config)
        }
    };

    if let Some(outcome) = outcome {
        span.in_scope(|| log_outcome(&outcome));
        // What the initiator's report would say, taken before the outcome moves into the queue.
        let report =
            job_report(&outcome).map(|(state, message)| (state, message.map(str::to_owned)));
        if let Err(error) = queue::complete(&shared.pool, id, token, outcome).await {
            // The lease runs out and the job goes back to the queue; handlers are idempotent.
            tracing::error!(parent: &span, %error, "cannot record the job outcome");
        } else if let Some((state, message)) = report {
            // A finished or failed job tells its initiator (ADM-031); the jobs API and screens
            // are step 1.11.
            report_job(&shared.pool, initiator_id, &kind, state, message).await;
        }
    } else {
        tracing::warn!(parent: &span, "lost the claim on the job; another worker will run it");
    }
    // Only our own entry: after a lost claim this runner may already run the job again.
    let mut in_flight = shared
        .in_flight
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if in_flight.get(&id) == Some(&token) {
        in_flight.remove(&id);
    }
}

/// What a job's end tells its initiator (ADM-031): the state and the failure's message, or
/// `None` when the job neither finished nor failed.
fn job_report(outcome: &Outcome) -> Option<(&'static str, Option<&str>)> {
    match outcome {
        Outcome::Finished => Some(("finished", None)),
        Outcome::Failed(error) => Some(("failed", Some(error.as_str()))),
        _ => None,
    }
}

/// Writes the initiator's `system` notification for a job that reached its end (ADM-031):
/// one row with the job, its state and the message, and the `inbox:` event that tells the
/// reader's open tabs (plan step 1.10 §3.6) in the same transaction.
async fn report_job(
    pool: &PgPool,
    initiator: Option<i64>,
    kind: &str,
    state: &'static str,
    message: Option<String>,
) {
    let Some(user) = initiator else {
        return;
    };
    let content = serde_json::json!({ "job": kind, "state": state, "message": message });
    let report = async {
        let mut tx = pool.begin().await?;
        let row = crate::notifications::system(&mut *tx, user, &content).await?;
        crate::live::publish(
            &mut tx,
            &[trackflow_domain::live::NewEvent {
                kind: trackflow_domain::live::EntityKind::Inbox,
                entity: user,
                version: row,
                change: trackflow_domain::live::Change::Created,
                issues: Vec::new(),
                projects: Vec::new(),
                boards: Vec::new(),
                patch: Some(trackflow_domain::live::Patch::Notification {
                    id: row,
                    event: "system".to_owned(),
                    issue: None,
                    author: String::new(),
                    excerpt: format!("{kind}: {state}"),
                    reasons: Vec::new(),
                }),
                guard: trackflow_domain::live::Guard::user(user),
            }],
        )
        .await?;
        tx.commit().await
    }
    .await;
    if let Err(error) = report {
        tracing::warn!(job = kind, user, %error, "cannot write the job report");
    }
}

/// Maps a handler result to what is stored. `None`: the claim was lost, store nothing.
fn interpret(
    ctx: &JobContext,
    result: Result<Result<(), JobError>, tokio::task::JoinError>,
    attempt: i32,
    config: &RunnerConfig,
) -> Option<Outcome> {
    if ctx.inner.claim_lost.load(Ordering::SeqCst) {
        return None;
    }
    Some(match result {
        Ok(Ok(())) => Outcome::Finished,
        Ok(Err(JobError::Fail(error))) => Outcome::Failed(error),
        Ok(Err(JobError::Retry(error))) => Outcome::Retry {
            error,
            delay: retry_delay(config.retry_base, attempt),
        },
        Ok(Err(JobError::Stopped)) if ctx.inner.cancel_requested.load(Ordering::SeqCst) => {
            Outcome::Cancelled
        }
        Ok(Err(JobError::Stopped)) if ctx.inner.stop.is_cancelled() => Outcome::Released,
        // Nobody asked it to stop: a handler bug, counted as a failed attempt.
        Ok(Err(JobError::Stopped)) => Outcome::Retry {
            error: "the handler stopped without being asked to".to_owned(),
            delay: retry_delay(config.retry_base, attempt),
        },
        Err(error) if error.is_panic() => Outcome::Failed("the job handler panicked".to_owned()),
        Err(_) => Outcome::Released,
    })
}

/// Aborts the task when dropped: when shutdown aborts a worker, its handler and lease renewal
/// stop with it.
struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn retry_delay(base: Duration, attempt: i32) -> Duration {
    let doublings = u32::try_from(attempt.saturating_sub(1))
        .unwrap_or(0)
        .min(16);
    base.saturating_mul(1 << doublings).min(MAX_RETRY_DELAY)
}

fn log_outcome(outcome: &Outcome) {
    match outcome {
        Outcome::Finished => tracing::info!("job finished"),
        Outcome::Cancelled => tracing::info!("job cancelled"),
        Outcome::Released => tracing::info!("job interrupted by shutdown, back in the queue"),
        Outcome::Failed(error) => tracing::warn!(%error, "job failed"),
        Outcome::Retry { error, delay } => {
            tracing::warn!(%error, retry_in = ?delay, "job failed, will retry")
        }
    }
}

async fn renew_lease(pool: PgPool, ctx: JobContext, lease: Duration) {
    let every = lease / 3;
    // The claim itself set the lease.
    let mut renewed = Instant::now();
    loop {
        tokio::time::sleep(every).await;
        let sent = Instant::now();
        match queue::renew(&pool, ctx.id(), ctx.inner.token, lease).await {
            Ok(answer) => {
                ctx.apply(answer);
                if answer.is_none() {
                    return;
                }
                renewed = sent;
            }
            Err(error) => {
                tracing::warn!(job = ctx.id(), %error, "cannot renew the job lease");
                if lease_runs_out(renewed.elapsed(), every, lease) {
                    // Stop before another worker may take the job over: two copies must not run.
                    tracing::warn!(
                        job = ctx.id(),
                        "giving the job up: its lease is about to expire"
                    );
                    ctx.apply(None);
                    return;
                }
            }
        }
    }
}

/// Whether the lease expires before the next renewal could extend it.
fn lease_runs_out(since_renewed: Duration, every: Duration, lease: Duration) -> bool {
    since_renewed + every >= lease
}

async fn wake_on_events(shared: Arc<Shared>, mut events: broadcast::Receiver<BusEvent>) {
    loop {
        let event = tokio::select! {
            () = shared.stop.cancelled() => return,
            event = events.recv() => event,
        };
        match event {
            Ok(BusEvent::Notified { channel, .. }) if channel == bus::JOBS => {
                shared.wake.notify_one();
            }
            Ok(BusEvent::Notified { .. }) => {}
            Ok(BusEvent::Resync) | Err(broadcast::error::RecvError::Lagged(_)) => {
                shared.wake.notify_one();
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn reap_expired(shared: Arc<Shared>) {
    loop {
        match queue::reap(&shared.pool).await {
            Ok(0) => {}
            Ok(count) => {
                tracing::warn!(count, "returned jobs of unresponsive workers to the queue");
                shared.wake.notify_one();
            }
            Err(error) => tracing::warn!(%error, "cannot check for expired job leases"),
        }
        tokio::select! {
            () = shared.stop.cancelled() => return,
            () = tokio::time::sleep(shared.config.reap_interval) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_gives_a_job_up_before_its_lease_expires() {
        let lease = Duration::from_secs(30);
        let every = lease / 3;
        assert!(!lease_runs_out(Duration::from_secs(10), every, lease));
        assert!(lease_runs_out(Duration::from_secs(20), every, lease));
        assert!(lease_runs_out(Duration::from_secs(31), every, lease));
    }

    #[test]
    fn retry_delay_doubles_up_to_an_hour() {
        let base = Duration::from_secs(10);
        assert_eq!(retry_delay(base, 1), Duration::from_secs(10));
        assert_eq!(retry_delay(base, 2), Duration::from_secs(20));
        assert_eq!(retry_delay(base, 4), Duration::from_secs(80));
        assert_eq!(retry_delay(base, 30), MAX_RETRY_DELAY);
        assert_eq!(retry_delay(Duration::ZERO, 3), Duration::ZERO);
    }
}
