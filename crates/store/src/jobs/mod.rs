//! Durable background jobs on PostgreSQL (ADM-031): the queue in the `jobs` table and the runner
//! that executes it. Design: architecture.md → Background jobs.

mod queue;
mod runner;

pub use queue::{
    CancelOutcome, Enqueue, Enqueued, Job, JobFilter, JobKind, JobState, Term, cancel,
    count_by_state, enqueue, get, list, reenqueue,
};
pub use runner::{Handlers, JobContext, JobError, JobFuture, Runner, RunnerConfig};
