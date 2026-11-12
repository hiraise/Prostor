//! The admin jobs API (ADM-031, ADM-032; plan step 1.11 §3.3): the list with its filters, and
//! the cancel and retry actions. Kind names and descriptions for people are the web's texts; the
//! API carries the machine kind.

use trackflow_domain::Actor;
use trackflow_domain::access::Access;
use trackflow_domain::access::permission::Permission;
use trackflow_store::PgPool;
use trackflow_store::jobs::{self, CancelOutcome, Job, JobFilter, JobState};

use crate::Error;

/// Lists jobs for the admin screen, newest first. Needs `system.read`.
pub async fn list(
    pool: &PgPool,
    actor: &Access,
    filter: JobFilter,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<Job>, Error> {
    actor.require(Permission::SystemRead)?;
    Ok(jobs::list(pool, filter, before, limit).await?)
}

/// Asks a job to stop (ADM-031): a queued one at once, a running one only when its kind may be
/// cancelled. Needs `system.write`; answers the job as it stands after the request.
pub async fn cancel(pool: &PgPool, actor: &Access, id: i64) -> Result<Job, Error> {
    actor.require(Permission::SystemWrite)?;
    match jobs::cancel(pool, id).await? {
        CancelOutcome::Cancelled | CancelOutcome::Requested => {}
        CancelOutcome::NotCancellable => {
            return Err(Error::Conflict(
                "this kind of job cannot be cancelled while it runs",
            ));
        }
        CancelOutcome::AlreadyFinal => {
            return Err(Error::Conflict(
                "the job has already finished, failed or been cancelled",
            ));
        }
        CancelOutcome::NotFound => return Err(Error::NotFound("job")),
    }
    jobs::get(pool, id).await?.ok_or(Error::NotFound("job"))
}

/// Queues a fresh copy of a failed job (ADM-032): the same kind and payload; the administrator
/// who asked becomes its initiator. Only a failed job can be retried. Needs `system.write`.
pub async fn retry(pool: &PgPool, actor: &Access, id: i64) -> Result<Job, Error> {
    actor.require(Permission::SystemWrite)?;
    let job = jobs::get(pool, id).await?.ok_or(Error::NotFound("job"))?;
    if job.state != JobState::Failed {
        return Err(Error::Conflict("only a failed job can be retried"));
    }
    let mut tx = pool.begin().await?;
    let id = jobs::reenqueue(&mut tx, &job, Actor::User(actor.user())).await?;
    tx.commit().await?;
    jobs::get(pool, id).await?.ok_or(Error::NotFound("job"))
}
