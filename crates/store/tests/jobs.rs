//! The durable job queue and runner (ADM-031). Runners run on the multi-thread runtime (see
//! `trackflow_testkit`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use trackflow_domain::Actor;
use trackflow_store::PgPool;
use trackflow_store::bus::Bus;
use trackflow_store::jobs::{
    self, CancelOutcome, Enqueue, Handlers, JobError, JobFilter, JobKind, JobState, Runner,
    RunnerConfig,
};
use trackflow_testkit::TestDb;

#[derive(Debug, Serialize, Deserialize)]
struct Count {
    units: i64,
}

impl JobKind for Count {
    const KIND: &'static str = "test.count";
}

/// Runs until told to stop; can be cancelled by users.
#[derive(Debug, Serialize, Deserialize)]
struct Wait;

impl JobKind for Wait {
    const KIND: &'static str = "test.wait";
    const CANCELLABLE: bool = true;
}

#[derive(Debug, Serialize, Deserialize)]
struct Flaky {
    failures: usize,
}

impl JobKind for Flaky {
    const KIND: &'static str = "test.flaky";
    const MAX_ATTEMPTS: i32 = 3;
}

#[derive(Debug, Serialize, Deserialize)]
struct Broken;

impl JobKind for Broken {
    const KIND: &'static str = "test.broken";
}

#[derive(Debug, Serialize, Deserialize)]
struct Unknown;

impl JobKind for Unknown {
    const KIND: &'static str = "test.unknown";
}

fn config() -> RunnerConfig {
    RunnerConfig {
        workers: 4,
        lease: Duration::from_secs(3),
        poll_interval: Duration::from_millis(50),
        reap_interval: Duration::from_millis(100),
        retry_base: Duration::ZERO,
        shutdown_grace: Duration::from_secs(5),
        ..RunnerConfig::default()
    }
}

/// Counts executed units; `Wait` blocks until stopped and signals when it started.
#[derive(Clone, Default)]
struct Probe {
    executed: Arc<AtomicUsize>,
    started: Arc<Notify>,
    flaky_runs: Arc<AtomicUsize>,
}

fn handlers(probe: &Probe) -> Handlers {
    let mut handlers = Handlers::new();
    let executed = Arc::clone(&probe.executed);
    handlers.add(move |ctx, job: Count| {
        let executed = Arc::clone(&executed);
        async move {
            for done in 1..=job.units {
                executed.fetch_add(1, Ordering::SeqCst);
                ctx.progress(done, Some(job.units), Some("counting"))
                    .await?;
            }
            Ok(())
        }
    });
    let started = Arc::clone(&probe.started);
    handlers.add(move |ctx, _: Wait| {
        let started = Arc::clone(&started);
        async move {
            started.notify_one();
            loop {
                tokio::select! {
                    () = ctx.stopped() => return Err(JobError::Stopped),
                    () = tokio::time::sleep(Duration::from_millis(30)) => {
                        ctx.progress(0, None, None).await?;
                    }
                }
            }
        }
    });
    let flaky_runs = Arc::clone(&probe.flaky_runs);
    handlers.add(move |_, job: Flaky| {
        let runs = flaky_runs.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            if runs <= job.failures {
                Err(JobError::Retry(format!("failure {runs}")))
            } else {
                Ok(())
            }
        }
    });
    handlers.add(|_, _: Broken| async { Err(JobError::Fail("cannot work".to_owned())) });
    handlers
}

/// A user to start jobs as.
async fn user(pool: &PgPool, username: &str) -> i64 {
    let mut conn = pool.acquire().await.unwrap();
    trackflow_store::users::insert(
        &mut conn,
        &trackflow_store::users::NewUser {
            username,
            full_name: username,
            email: None,
            builtin: None,
            invited: false,
            disabled: false,
        },
    )
    .await
    .unwrap()
}

async fn enqueue<J: JobKind>(pool: &PgPool, job: &J, options: Enqueue) -> i64 {
    let mut conn = pool.acquire().await.unwrap();
    jobs::enqueue(&mut conn, job, options).await.unwrap().id
}

async fn wait_for_state(pool: &PgPool, id: i64, state: JobState) -> jobs::Job {
    for _ in 0..200 {
        let job = jobs::get(pool, id).await.unwrap().unwrap();
        if job.state == state {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let job = jobs::get(pool, id).await.unwrap().unwrap();
    panic!("job {id} did not reach {state:?}: {job:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_a_queued_job_runs_and_records_progress() {
    let db = TestDb::new().await;
    let probe = Probe::default();
    let runner = Runner::start(db.pool().clone(), handlers(&probe), config(), None);
    let anna = user(db.pool(), "anna").await;

    let id = enqueue(
        db.pool(),
        &Count { units: 3 },
        Enqueue {
            initiator: Actor::User(anna),
            ..Enqueue::default()
        },
    )
    .await;
    let job = wait_for_state(db.pool(), id, JobState::Finished).await;
    assert_eq!(probe.executed.load(Ordering::SeqCst), 3);
    assert_eq!(job.kind, "test.count");
    assert_eq!(job.initiator_id, Some(anna));
    assert_eq!(job.attempts, 1);
    assert_eq!((job.progress_done, job.progress_total), (3, Some(3)));
    assert_eq!(job.status_text.as_deref(), Some("counting"));
    assert!(job.started_at.is_some() && job.finished_at.is_some());
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_a_notification_wakes_an_idle_runner_at_once() {
    let db = TestDb::new().await;
    let stop = CancellationToken::new();
    let (bus, _task) = Bus::start(db.pool().clone(), stop.clone()).await.unwrap();
    let probe = Probe::default();
    let slow_poll = RunnerConfig {
        poll_interval: Duration::from_secs(60),
        ..config()
    };
    let runner = Runner::start(
        db.pool().clone(),
        handlers(&probe),
        slow_poll,
        Some(bus.subscribe()),
    );
    // Let the runner find the queue empty and go idle first.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let id = enqueue(db.pool(), &Count { units: 1 }, Enqueue::default()).await;
    wait_for_state(db.pool(), id, JobState::Finished).await;
    runner.shutdown().await;
    stop.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_temporary_failures_are_retried_until_attempts_run_out() {
    let db = TestDb::new().await;
    let probe = Probe::default();
    let runner = Runner::start(db.pool().clone(), handlers(&probe), config(), None);

    let recovers = enqueue(db.pool(), &Flaky { failures: 2 }, Enqueue::default()).await;
    let job = wait_for_state(db.pool(), recovers, JobState::Finished).await;
    assert_eq!(job.attempts, 3);
    assert_eq!(job.last_error.as_deref(), Some("failure 2"));

    probe.flaky_runs.store(0, Ordering::SeqCst);
    let gives_up = enqueue(db.pool(), &Flaky { failures: 10 }, Enqueue::default()).await;
    let job = wait_for_state(db.pool(), gives_up, JobState::Failed).await;
    assert_eq!(job.attempts, 3);
    assert_eq!(job.last_error.as_deref(), Some("failure 3"));
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_permanent_failures_and_bad_payloads_fail_at_once() {
    let db = TestDb::new().await;
    let runner = Runner::start(
        db.pool().clone(),
        handlers(&Probe::default()),
        config(),
        None,
    );

    let broken = enqueue(db.pool(), &Broken, Enqueue::default()).await;
    let job = wait_for_state(db.pool(), broken, JobState::Failed).await;
    assert_eq!(
        (job.attempts, job.last_error.as_deref()),
        (1, Some("cannot work"))
    );

    // A payload that does not match the kind's type. Changed in the same transaction, so no
    // runner sees the job before.
    let mut tx = db.pool().begin().await.unwrap();
    let odd = jobs::enqueue(&mut tx, &Broken, Enqueue::default())
        .await
        .unwrap()
        .id;
    sqlx::query(
        "update jobs set kind = 'test.count', payload = '{\"units\": \"many\"}' where id = $1",
    )
    .bind(odd)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let job = wait_for_state(db.pool(), odd, JobState::Failed).await;
    assert!(job.last_error.unwrap().starts_with("invalid payload"));
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_unknown_kinds_stay_queued_for_a_node_that_knows_them() {
    let db = TestDb::new().await;
    let runner = Runner::start(
        db.pool().clone(),
        handlers(&Probe::default()),
        config(),
        None,
    );
    let id = enqueue(db.pool(), &Unknown, Enqueue::default()).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let job = jobs::get(db.pool(), id).await.unwrap().unwrap();
    assert_eq!((job.state, job.attempts), (JobState::Queued, 0));
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_cancellation_of_queued_running_and_uncancellable_jobs() {
    let db = TestDb::new().await;
    let probe = Probe::default();

    // Queued: cancelled at once, never runs.
    let queued = enqueue(db.pool(), &Wait, Enqueue::default()).await;
    assert_eq!(
        jobs::cancel(db.pool(), queued).await.unwrap(),
        CancelOutcome::Cancelled
    );

    let runner = Runner::start(db.pool().clone(), handlers(&probe), config(), None);
    let running = enqueue(db.pool(), &Wait, Enqueue::default()).await;
    probe.started.notified().await;
    assert_eq!(
        jobs::cancel(db.pool(), running).await.unwrap(),
        CancelOutcome::Requested
    );
    let job = wait_for_state(db.pool(), running, JobState::Cancelled).await;
    assert!(job.finished_at.is_some());
    assert_eq!(
        jobs::cancel(db.pool(), running).await.unwrap(),
        CancelOutcome::AlreadyFinal
    );

    let plain = enqueue(db.pool(), &Unknown, Enqueue::default()).await;
    assert_eq!(
        jobs::cancel(db.pool(), plain).await.unwrap(),
        CancelOutcome::NotCancellable
    );
    assert_eq!(
        jobs::cancel(db.pool(), 987_654).await.unwrap(),
        CancelOutcome::NotFound
    );
    let job = jobs::get(db.pool(), queued).await.unwrap().unwrap();
    assert_eq!((job.state, job.attempts), (JobState::Cancelled, 0));
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_a_unique_key_allows_one_active_job() {
    let db = TestDb::new().await;
    let options = || Enqueue {
        unique_key: Some("rebuild-index".to_owned()),
        ..Enqueue::default()
    };
    let mut conn = db.pool().acquire().await.unwrap();
    let first = jobs::enqueue(&mut conn, &Unknown, options()).await.unwrap();
    let again = jobs::enqueue(&mut conn, &Unknown, options()).await.unwrap();
    assert!(first.created);
    assert_eq!(
        again,
        jobs::Enqueued {
            id: first.id,
            created: false
        }
    );

    sqlx::query("update jobs set state = 'finished' where id = $1")
        .bind(first.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    let after = jobs::enqueue(&mut conn, &Unknown, options()).await.unwrap();
    assert!(after.created);
    assert_ne!(after.id, first.id);
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_a_job_only_becomes_visible_when_its_transaction_commits() {
    let db = TestDb::new().await;
    let mut tx = db.pool().begin().await.unwrap();
    let id = jobs::enqueue(&mut tx, &Unknown, Enqueue::default())
        .await
        .unwrap()
        .id;
    tx.rollback().await.unwrap();
    assert!(jobs::get(db.pool(), id).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_jobs_survive_a_restart() {
    let db = TestDb::new().await;
    let probe = Probe::default();
    let first = Runner::start(db.pool().clone(), handlers(&probe), config(), None);
    let id = enqueue(db.pool(), &Wait, Enqueue::default()).await;
    probe.started.notified().await;

    // Shutdown stops the handler and returns the job to the queue without using up an attempt.
    first.shutdown().await;
    let job = jobs::get(db.pool(), id).await.unwrap().unwrap();
    assert_eq!((job.state, job.attempts), (JobState::Queued, 0));

    let second = Runner::start(db.pool().clone(), handlers(&probe), config(), None);
    probe.started.notified().await;
    let job = wait_for_state(db.pool(), id, JobState::Running).await;
    assert_eq!(job.attempts, 1);
    jobs::cancel(db.pool(), id).await.unwrap();
    wait_for_state(db.pool(), id, JobState::Cancelled).await;
    second.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_jobs_of_a_crashed_worker_return_to_the_queue() {
    let db = TestDb::new().await;
    let id = enqueue(db.pool(), &Count { units: 2 }, Enqueue::default()).await;
    // A worker that took the job and vanished: running, with an expired lease.
    sqlx::query(
        "update jobs set state = 'running', attempts = 1, worker = 'gone',
                lease_until = now() - interval '1 second'
         where id = $1",
    )
    .bind(id)
    .execute(db.pool())
    .await
    .unwrap();

    let probe = Probe::default();
    let runner = Runner::start(db.pool().clone(), handlers(&probe), config(), None);
    let job = wait_for_state(db.pool(), id, JobState::Finished).await;
    assert_eq!(job.attempts, 2);
    assert_eq!(probe.executed.load(Ordering::SeqCst), 2);
    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_each_job_runs_exactly_once_across_nodes() {
    let db = TestDb::new().await;
    let probe = Probe::default();
    let mut ids = Vec::new();
    for _ in 0..40 {
        ids.push(enqueue(db.pool(), &Count { units: 1 }, Enqueue::default()).await);
    }
    let nodes: Vec<Runner> = (0..3)
        .map(|_| Runner::start(db.pool().clone(), handlers(&probe), config(), None))
        .collect();
    for id in &ids {
        let job = wait_for_state(db.pool(), *id, JobState::Finished).await;
        assert_eq!(job.attempts, 1);
    }
    assert_eq!(probe.executed.load(Ordering::SeqCst), 40);
    for node in nodes {
        node.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn adm_031_lists_page_newest_first_and_counts_by_state() {
    let db = TestDb::new().await;
    let (anna, boris) = (
        user(db.pool(), "anna").await,
        user(db.pool(), "boris").await,
    );
    let mut ids = Vec::new();
    for initiator in [anna, boris, anna, anna, boris] {
        let options = Enqueue {
            initiator: Actor::User(initiator),
            ..Enqueue::default()
        };
        ids.push(enqueue(db.pool(), &Unknown, options).await);
    }

    let page = jobs::list(db.pool(), JobFilter::default(), None, 3)
        .await
        .unwrap();
    let got: Vec<i64> = page.iter().map(|job| job.id).collect();
    assert_eq!(got, [ids[4], ids[3], ids[2]]);
    let rest = jobs::list(db.pool(), JobFilter::default(), Some(ids[2]), 3)
        .await
        .unwrap();
    let got: Vec<i64> = rest.iter().map(|job| job.id).collect();
    assert_eq!(got, [ids[1], ids[0]]);

    let mine = jobs::list(
        db.pool(),
        JobFilter {
            initiator: Some(anna),
            ..JobFilter::default()
        },
        None,
        10,
    )
    .await
    .unwrap();
    let got: Vec<i64> = mine.iter().map(|job| job.id).collect();
    assert_eq!(got, [ids[3], ids[2], ids[0]]);

    let counts = jobs::count_by_state(db.pool()).await.unwrap();
    assert_eq!(counts, [(JobState::Queued, 5)]);
}
