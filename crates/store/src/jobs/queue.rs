//! The `jobs` table: enqueueing, claiming, outcomes and reading.

use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use trackflow_domain::{Actor, UserId};

use crate::bus;

/// A kind of job: its payload type, stable name and policy.
pub trait JobKind: Serialize + DeserializeOwned + Send + 'static {
    /// Stable name stored with each job, such as `projects.delete`. Never rename a kind that may
    /// still be queued somewhere.
    const KIND: &'static str;
    /// Whether a user may cancel the job while it runs. Only for jobs that can stop midway safely.
    const CANCELLABLE: bool = false;
    /// How many times the job runs before it is marked failed.
    const MAX_ATTEMPTS: i32 = 5;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobState {
    Queued,
    Running,
    Finished,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Finished => "finished",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self, sqlx::Error> {
        Ok(match value {
            "queued" => JobState::Queued,
            "running" => JobState::Running,
            "finished" => JobState::Finished,
            "failed" => JobState::Failed,
            "cancelled" => JobState::Cancelled,
            other => {
                return Err(sqlx::Error::Decode(
                    format!("unknown job state {other:?}").into(),
                ));
            }
        })
    }

    /// Finished, failed and cancelled jobs never change again.
    pub fn is_final(self) -> bool {
        matches!(
            self,
            JobState::Finished | JobState::Failed | JobState::Cancelled
        )
    }
}

/// Options of a new job.
#[derive(Debug, Clone, Default)]
pub struct Enqueue {
    /// Who started it; notified about the result.
    pub initiator: Actor,
    /// At most one queued or running job per key: enqueueing again returns the existing job.
    pub unique_key: Option<String>,
    /// Higher runs first.
    pub priority: i16,
    /// Not before this time; `None` means now.
    pub run_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Enqueued {
    pub id: i64,
    /// `false` when a job with the same unique key was already queued or running.
    pub created: bool,
}

/// Queues a job as part of the caller's transaction: it becomes visible to runners, and they are
/// woken, only when that transaction commits.
pub async fn enqueue<J: JobKind>(
    conn: &mut PgConnection,
    job: &J,
    options: Enqueue,
) -> Result<Enqueued, sqlx::Error> {
    let payload = serde_json::to_value(job).map_err(|error| sqlx::Error::Encode(error.into()))?;
    // A job with the same key may finish between the insert and the lookup; then insert again.
    for _ in 0..3 {
        let inserted = sqlx::query_scalar!(
            "insert into jobs (kind, payload, initiator_id, unique_key, cancellable, priority,
                               max_attempts, run_at)
             values ($1, $2, $3, $4, $5, $6, $7, coalesce($8, now()))
             on conflict (unique_key) where state in ('queued', 'running') do nothing
             returning id",
            J::KIND,
            payload,
            options.initiator.user_id(),
            options.unique_key,
            J::CANCELLABLE,
            options.priority,
            J::MAX_ATTEMPTS,
            options.run_at,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(id) = inserted {
            bus::notify(conn, bus::JOBS, J::KIND).await?;
            return Ok(Enqueued { id, created: true });
        }

        let existing = sqlx::query_scalar!(
            "select id from jobs where unique_key = $1 and state in ('queued', 'running')",
            options.unique_key,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(id) = existing {
            return Ok(Enqueued { id, created: false });
        }
    }
    Err(sqlx::Error::Protocol(
        "a job with the same unique key keeps appearing and finishing".into(),
    ))
}

/// A job claimed by a worker.
#[derive(Debug)]
pub(crate) struct Claimed {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    /// Which run this is, counting from 1.
    pub attempt: i32,
    /// Identifies this claim; guards every later update of the job.
    pub token: i64,
    /// Who asked for the job; its report is theirs (ADM-031). `None` when the server itself
    /// started it.
    pub initiator_id: Option<i64>,
}

/// Takes the next due job of one of `kinds`, skipping jobs other workers hold.
pub(crate) async fn claim(
    pool: &PgPool,
    kinds: &[String],
    worker: &str,
    lease: Duration,
) -> Result<Option<Claimed>, sqlx::Error> {
    sqlx::query_as!(
        Claimed,
        r#"update jobs
           set state = 'running', attempts = attempts + 1, claim_token = claim_token + 1,
               worker = $2,
               lease_until = now() + make_interval(secs => $3),
               started_at = coalesce(started_at, now()), updated_at = now()
           where id = (
               select id from jobs
               where state = 'queued' and run_at <= now() and kind = any($1)
               order by priority desc, run_at, id
               limit 1
               for update skip locked
           )
           returning id, kind, payload, attempts as attempt, claim_token as token,
                     initiator_id"#,
        kinds,
        worker,
        lease.as_secs_f64(),
    )
    .fetch_optional(pool)
    .await
}

/// Extends the lease of a claimed job. `None`: the claim is no longer ours (it expired and the job
/// went back to the queue); otherwise whether a user asked to cancel the job.
pub(crate) async fn renew(
    pool: &PgPool,
    id: i64,
    token: i64,
    lease: Duration,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar!(
        "update jobs set lease_until = now() + make_interval(secs => $3), updated_at = now()
         where id = $1 and claim_token = $2 and state = 'running'
         returning cancel_requested",
        id,
        token,
        lease.as_secs_f64(),
    )
    .fetch_optional(pool)
    .await
}

/// Records progress of a claimed job; same answer as [`renew`].
pub(crate) async fn set_progress(
    pool: &PgPool,
    id: i64,
    token: i64,
    done: i64,
    total: Option<i64>,
    status: Option<&str>,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar!(
        "update jobs
         set progress_done = $3, progress_total = $4, status_text = coalesce($5, status_text),
             updated_at = now()
         where id = $1 and claim_token = $2 and state = 'running'
         returning cancel_requested",
        id,
        token,
        done,
        total,
        status,
    )
    .fetch_optional(pool)
    .await
}

/// How a claimed job ended.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    Finished,
    Cancelled,
    /// Will not succeed; no more attempts.
    Failed(String),
    /// Try again after `delay`, or fail when out of attempts.
    Retry {
        error: String,
        delay: Duration,
    },
    /// Stopped by shutdown before it finished: back to the queue, the attempt does not count.
    Released,
}

/// Writes the outcome of a claim. A cancellation requested meanwhile wins over a retry or release.
/// Does nothing if the claim is no longer ours.
pub(crate) async fn complete(
    pool: &PgPool,
    id: i64,
    token: i64,
    outcome: Outcome,
) -> Result<(), sqlx::Error> {
    match outcome {
        Outcome::Finished => {
            sqlx::query!(
                "update jobs set state = 'finished', finished_at = now(), lease_until = null,
                     updated_at = now()
                 where id = $1 and claim_token = $2 and state = 'running'",
                id,
                token,
            )
            .execute(pool)
            .await?;
        }
        Outcome::Cancelled => {
            sqlx::query!(
                "update jobs set state = 'cancelled', finished_at = now(), lease_until = null,
                     updated_at = now()
                 where id = $1 and claim_token = $2 and state = 'running'",
                id,
                token,
            )
            .execute(pool)
            .await?;
        }
        Outcome::Failed(error) => {
            sqlx::query!(
                "update jobs set state = 'failed', last_error = $3, finished_at = now(),
                     lease_until = null, updated_at = now()
                 where id = $1 and claim_token = $2 and state = 'running'",
                id,
                token,
                error,
            )
            .execute(pool)
            .await?;
        }
        Outcome::Retry { error, delay } => {
            // Jitter of ±25% keeps retries of many jobs that failed together apart.
            sqlx::query!(
                "update jobs
                 set state = case when cancel_requested then 'cancelled'
                                  when attempts >= max_attempts then 'failed'
                                  else 'queued' end,
                     finished_at = case when cancel_requested or attempts >= max_attempts
                                        then now() end,
                     run_at = now() + make_interval(secs => $4 * (0.75 + random() * 0.5)),
                     last_error = $3, lease_until = null, updated_at = now()
                 where id = $1 and claim_token = $2 and state = 'running'",
                id,
                token,
                error,
                delay.as_secs_f64(),
            )
            .execute(pool)
            .await?;
        }
        Outcome::Released => {
            sqlx::query!(
                "update jobs
                 set state = case when cancel_requested then 'cancelled' else 'queued' end,
                     finished_at = case when cancel_requested then now() end,
                     attempts = attempts - 1, run_at = now(), lease_until = null,
                     updated_at = now()
                 where id = $1 and claim_token = $2 and state = 'running'",
                id,
                token,
            )
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// Returns running jobs whose lease ran out (their worker crashed or lost the database) to the
/// queue, or fails them when they are out of attempts. Returns how many jobs it touched.
pub(crate) async fn reap(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        "update jobs
         set state = case when cancel_requested then 'cancelled'
                          when attempts >= max_attempts then 'failed'
                          else 'queued' end,
             finished_at = case when cancel_requested or attempts >= max_attempts then now() end,
             last_error = 'the worker running the job stopped responding',
             run_at = now(), lease_until = null, updated_at = now()
         where state = 'running' and lease_until < now()",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Result of a cancellation request.
/// Empties the payload of a job this worker still holds (`JobContext::forget_payload`).
pub(crate) async fn clear_payload(pool: &PgPool, id: i64, token: i64) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update jobs set payload = '{}', updated_at = now()
         where id = $1 and claim_token = $2 and state = 'running'",
        id,
        token,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// It was waiting in the queue and is now cancelled.
    Cancelled,
    /// It is running and stops at its next check.
    Requested,
    /// This kind of job cannot be cancelled.
    NotCancellable,
    /// It has already finished, failed or been cancelled.
    AlreadyFinal,
    NotFound,
}

/// Asks a job to stop. Queued jobs are cancelled at once; running ones learn it at their next
/// progress report or lease renewal.
pub async fn cancel(pool: &PgPool, id: i64) -> Result<CancelOutcome, sqlx::Error> {
    let changed = sqlx::query_scalar!(
        "update jobs
         set cancel_requested = true,
             state = case when state = 'queued' then 'cancelled' else state end,
             finished_at = case when state = 'queued' then now() else finished_at end,
             updated_at = now()
         where id = $1 and cancellable and state in ('queued', 'running')
         returning state",
        id,
    )
    .fetch_optional(pool)
    .await?;
    if let Some(state) = changed {
        return Ok(if state == "cancelled" {
            CancelOutcome::Cancelled
        } else {
            CancelOutcome::Requested
        });
    }

    let job = sqlx::query!("select state, cancellable from jobs where id = $1", id)
        .fetch_optional(pool)
        .await?;
    Ok(match job {
        None => CancelOutcome::NotFound,
        Some(job) if JobState::parse(&job.state)?.is_final() => CancelOutcome::AlreadyFinal,
        Some(_) => CancelOutcome::NotCancellable,
    })
}

/// Queues a fresh copy of a failed job (ADM-032): the same kind, payload and policy; the person
/// who asked for the retry becomes the initiator. When a copy with the same unique key is already
/// queued or running, that copy's id comes back and nothing is added.
pub async fn reenqueue(
    conn: &mut PgConnection,
    job: &Job,
    initiator: Actor,
) -> Result<i64, sqlx::Error> {
    let inserted = sqlx::query_scalar!(
        "insert into jobs (kind, payload, initiator_id, unique_key, cancellable, priority,
                           max_attempts, run_at)
         values ($1, $2, $3, $4, $5, 0, $6, now())
         on conflict (unique_key) where state in ('queued', 'running') do nothing
         returning id",
        job.kind,
        job.payload,
        initiator.user_id(),
        job.unique_key,
        job.cancellable,
        job.max_attempts,
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(id) = inserted {
        bus::notify(conn, bus::JOBS, &job.kind).await?;
        return Ok(id);
    }
    // A job without a unique key always takes the insert path above.
    sqlx::query_scalar!(
        "select id from jobs where unique_key = $1 and state in ('queued', 'running')",
        job.unique_key,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| {
        sqlx::Error::Protocol(
            "a retried job with the same key keeps appearing and finishing".into(),
        )
    })
}

/// A job as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    pub state: JobState,
    /// `None` when the server itself started it.
    pub initiator_id: Option<UserId>,
    /// At most one queued or running job per key.
    pub unique_key: Option<String>,
    pub cancellable: bool,
    pub cancel_requested: bool,
    pub attempts: i32,
    pub max_attempts: i32,
    pub run_at: OffsetDateTime,
    pub progress_done: i64,
    pub progress_total: Option<i64>,
    pub status_text: Option<String>,
    pub last_error: Option<String>,
    pub created_at: OffsetDateTime,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
}

struct JobRow {
    id: i64,
    kind: String,
    payload: Value,
    state: String,
    initiator_id: Option<i64>,
    unique_key: Option<String>,
    cancellable: bool,
    cancel_requested: bool,
    attempts: i32,
    max_attempts: i32,
    run_at: OffsetDateTime,
    progress_done: i64,
    progress_total: Option<i64>,
    status_text: Option<String>,
    last_error: Option<String>,
    created_at: OffsetDateTime,
    started_at: Option<OffsetDateTime>,
    finished_at: Option<OffsetDateTime>,
}

impl TryFrom<JobRow> for Job {
    type Error = sqlx::Error;

    fn try_from(row: JobRow) -> Result<Self, Self::Error> {
        Ok(Job {
            id: row.id,
            kind: row.kind,
            payload: row.payload,
            state: JobState::parse(&row.state)?,
            initiator_id: row.initiator_id,
            unique_key: row.unique_key,
            cancellable: row.cancellable,
            cancel_requested: row.cancel_requested,
            attempts: row.attempts,
            max_attempts: row.max_attempts,
            run_at: row.run_at,
            progress_done: row.progress_done,
            progress_total: row.progress_total,
            status_text: row.status_text,
            last_error: row.last_error,
            created_at: row.created_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
        })
    }
}

pub async fn get(pool: &PgPool, id: i64) -> Result<Option<Job>, sqlx::Error> {
    sqlx::query_as!(
        JobRow,
        "select id, kind, payload, state, initiator_id, unique_key, cancellable,
                cancel_requested, attempts, max_attempts, run_at, progress_done, progress_total,
                status_text, last_error, created_at, started_at, finished_at
         from jobs where id = $1",
        id,
    )
    .fetch_optional(pool)
    .await?
    .map(Job::try_from)
    .transpose()
}

/// A word of the admin filter (ADM-032): matched against the kind and the initiator's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// Jobs whose kind or initiator matches.
    Include(String),
    /// Jobs whose kind and initiator do not match.
    Exclude(String),
}

impl Term {
    /// Splits a query into words; a leading `-` excludes. Empty words and a bare `-` drop out.
    pub fn parse(query: &str) -> Vec<Term> {
        query
            .split_whitespace()
            .filter_map(|word| match word.strip_prefix('-') {
                Some(term) if !term.is_empty() => Some(Term::Exclude(term.to_owned())),
                Some(_) => None,
                None => Some(Term::Include(word.to_owned())),
            })
            .collect()
    }
}

/// Which jobs to list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobFilter {
    /// Only jobs started by this user.
    pub initiator: Option<UserId>,
    /// Only jobs in this state (ADM-031).
    pub state: Option<JobState>,
    /// Words matched against the kind and the initiator's name (ADM-032). Unindexed: an
    /// admin-only screen over a small table, acceptable.
    pub terms: Vec<Term>,
}

/// Jobs newest first: up to `limit` jobs with an ID below `before` (keyset pagination). Without
/// terms each filter combination keeps its own query, so the index of that combination serves it
/// (the state filter rides `jobs_state_idx`); the term match joins the initiators.
pub async fn list(
    pool: &PgPool,
    filter: JobFilter,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<Job>, sqlx::Error> {
    if filter.terms.is_empty() {
        return list_indexed(pool, filter, before, limit).await;
    }
    let (mut include, mut exclude) = (Vec::new(), Vec::new());
    for term in &filter.terms {
        match term {
            Term::Include(word) => include.push(format!("%{word}%")),
            Term::Exclude(word) => exclude.push(format!("%{word}%")),
        }
    }
    let include = (!include.is_empty()).then_some(include);
    let exclude = (!exclude.is_empty()).then_some(exclude);
    let rows = sqlx::query_as!(
        JobRow,
        "select j.id, j.kind, j.payload, j.state, j.initiator_id, j.unique_key, j.cancellable,
                j.cancel_requested, j.attempts, j.max_attempts, j.run_at, j.progress_done,
                j.progress_total, j.status_text, j.last_error, j.created_at, j.started_at,
                j.finished_at
         from jobs j left join users u on u.id = j.initiator_id
         where j.id < $1
           and ($2::bigint is null or j.initiator_id = $2)
           and ($3::text is null or j.state = $3)
           and not exists (
               select 1 from unnest($4::text[]) as t(pattern)
               where not (j.kind ilike t.pattern
                          or coalesce(u.full_name, '') ilike t.pattern
                          or coalesce(u.username, '') ilike t.pattern))
           and not exists (
               select 1 from unnest($5::text[]) as t(pattern)
               where j.kind ilike t.pattern
                  or coalesce(u.full_name, '') ilike t.pattern
                  or coalesce(u.username, '') ilike t.pattern)
         order by j.id desc limit $6",
        before.unwrap_or(i64::MAX),
        filter.initiator,
        filter.state.map(|state| state.as_str()),
        include.as_deref(),
        exclude.as_deref(),
        limit,
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(Job::try_from).collect()
}

/// The list without terms: one fixed query per filter combination, each on its index.
async fn list_indexed(
    pool: &PgPool,
    filter: JobFilter,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<Job>, sqlx::Error> {
    // Separate queries per filter keep each one on its index; `or $1 is null` would not.
    let before = before.unwrap_or(i64::MAX);
    let rows = match (filter.initiator, filter.state) {
        (Some(initiator), Some(state)) => {
            sqlx::query_as!(
                JobRow,
                "select id, kind, payload, state, initiator_id, unique_key, cancellable,
                        cancel_requested, attempts, max_attempts, run_at, progress_done,
                        progress_total, status_text, last_error, created_at, started_at,
                        finished_at
                 from jobs where initiator_id = $1 and state = $2 and id < $3
                 order by id desc limit $4",
                initiator,
                state.as_str(),
                before,
                limit,
            )
            .fetch_all(pool)
            .await?
        }
        (Some(initiator), None) => {
            sqlx::query_as!(
                JobRow,
                "select id, kind, payload, state, initiator_id, unique_key, cancellable,
                        cancel_requested, attempts, max_attempts, run_at, progress_done,
                        progress_total, status_text, last_error, created_at, started_at,
                        finished_at
                 from jobs where initiator_id = $1 and id < $2 order by id desc limit $3",
                initiator,
                before,
                limit,
            )
            .fetch_all(pool)
            .await?
        }
        (None, Some(state)) => {
            sqlx::query_as!(
                JobRow,
                "select id, kind, payload, state, initiator_id, unique_key, cancellable,
                        cancel_requested, attempts, max_attempts, run_at, progress_done,
                        progress_total, status_text, last_error, created_at, started_at,
                        finished_at
                 from jobs where state = $1 and id < $2 order by id desc limit $3",
                state.as_str(),
                before,
                limit,
            )
            .fetch_all(pool)
            .await?
        }
        (None, None) => {
            sqlx::query_as!(
                JobRow,
                "select id, kind, payload, state, initiator_id, unique_key, cancellable,
                        cancel_requested, attempts, max_attempts, run_at, progress_done,
                        progress_total, status_text, last_error, created_at, started_at,
                        finished_at
                 from jobs where id < $1 order by id desc limit $2",
                before,
                limit,
            )
            .fetch_all(pool)
            .await?
        }
    };
    rows.into_iter().map(Job::try_from).collect()
}

/// Number of jobs per state, for the metrics page (ADM-034).
pub async fn count_by_state(pool: &PgPool) -> Result<Vec<(JobState, i64)>, sqlx::Error> {
    sqlx::query!(r#"select state, count(*) as "count!" from jobs group by state order by state"#)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| Ok((JobState::parse(&row.state)?, row.count)))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use trackflow_testkit::TestDb;

    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Probe;

    impl JobKind for Probe {
        const KIND: &'static str = "test.probe";
    }

    const LEASE: Duration = Duration::from_secs(30);

    #[tokio::test]
    async fn adm_031_late_writes_of_an_old_claim_change_nothing() {
        let db = TestDb::new().await;
        let pool = db.pool();
        let mut conn = pool.acquire().await.unwrap();
        let id = enqueue(&mut conn, &Probe, Enqueue::default())
            .await
            .unwrap()
            .id;
        drop(conn);
        let kinds = vec![Probe::KIND.to_owned()];

        let first = claim(pool, &kinds, "a", LEASE).await.unwrap().unwrap();
        complete(pool, id, first.token, Outcome::Released)
            .await
            .unwrap();
        let second = claim(pool, &kinds, "b", LEASE).await.unwrap().unwrap();
        // The interrupted run did not count, but the new claim is a different one.
        assert_eq!((first.attempt, second.attempt), (1, 1));
        assert_ne!(first.token, second.token);

        // The first worker's renewal, progress and result arrive late and are ignored.
        assert_eq!(renew(pool, id, first.token, LEASE).await.unwrap(), None);
        assert_eq!(
            set_progress(pool, id, first.token, 5, None, Some("late"))
                .await
                .unwrap(),
            None
        );
        complete(pool, id, first.token, Outcome::Failed("late".to_owned()))
            .await
            .unwrap();
        let job = get(pool, id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Running);
        assert_eq!((job.progress_done, job.last_error), (0, None));

        complete(pool, id, second.token, Outcome::Finished)
            .await
            .unwrap();
        let job = get(pool, id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Finished);
    }
}
