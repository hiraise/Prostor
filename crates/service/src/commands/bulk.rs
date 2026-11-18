//! Bulk application over a query (plan §4.7, CMD-026): «apply this command to every issue the
//! query lists». A job of kind `commands` walks the query's IDs in keyset pages, applies the
//! line per issue — each issue stands alone with its own result (§1) — reports progress and is
//! cancelable between issues. The synchronous half (explicit IDs, at most
//! [`super::ISSUES_MAX`]) is [`super::apply`].

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use taskmaster_domain::access::Access;
use taskmaster_domain::issues::history::Origin;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{IssueId, ProjectId, UserId};
use taskmaster_store::jobs::{Enqueue, Enqueued, JobContext, JobError, JobKind};
use taskmaster_store::{PgPool, issues as store_issues, jobs};

use super::ApplyInput;
use crate::access;
use crate::search::{self, SearchInput};
use crate::{Error, Result};

/// Issues one page of the walk reads (§4.7's keyset pages).
const PAGE: u32 = 100;
/// Issues one progress report covers: one database write per batch, not per issue.
const REPORT: usize = 10;
/// Failures kept whole in the job's results; the count keeps the rest.
const FAILURES_MAX: usize = 200;

/// The job's payload (§4.8's `POST /commands/bulk`): the line, the query that names the
/// issues, and the extras the synchronous apply takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BulkWork {
    pub actor: UserId,
    pub text: String,
    /// The query every matching issue of which the line applies to; empty means the project's
    /// every issue.
    pub query: String,
    pub project: Option<ProjectId>,
    pub hide_resolved: bool,
    pub time_zone: Option<String>,
    pub comment: Option<String>,
    pub comment_visibility: Option<crate::issues::AudienceInput>,
    pub silent: bool,
    pub create_missing: bool,
    pub run_as: Option<UserId>,
}

impl JobKind for BulkWork {
    const KIND: &'static str = "commands";
    const CANCELLABLE: bool = true;
    const MAX_ATTEMPTS: i32 = 3;
}

/// Queues the bulk job (§4.7): the shape is checked here, the command line itself by the job,
/// which fails fast when it does not resolve.
pub async fn enqueue(pool: &PgPool, actor: &Access, work: BulkWork) -> Result<Enqueued> {
    if work.text.trim().is_empty() {
        return Err(Error::field(
            "text",
            Problem::Required,
            "name the command to apply",
        ));
    }
    if work.query.trim().is_empty() && work.project.is_none() {
        return Err(Error::field(
            "query",
            Problem::Required,
            "name the query whose issues the command applies to",
        ));
    }
    let mut conn = pool.acquire().await?;
    jobs::enqueue(
        &mut conn,
        &work,
        Enqueue {
            initiator: taskmaster_domain::Actor::User(actor.user()),
            ..Enqueue::default()
        },
    )
    .await
    .map_err(Error::from)
}

/// The job's progress and results, for its owner alone (§4.7: `GET /commands/bulk/{id}`).
#[derive(Debug, Clone, Serialize)]
pub struct BulkProgress {
    pub id: i64,
    /// `queued`, `running`, `finished`, `cancelled`, `failed`.
    pub state: String,
    pub done: i64,
    pub total: Option<i64>,
    pub failed: i64,
    /// `{issue, code, detail}` per failed issue, capped at [`FAILURES_MAX`].
    pub failures: Vec<Value>,
    pub cancel_requested: bool,
    pub last_error: Option<String>,
}

/// Reads one bulk job's progress; nobody else's (§4.7).
pub async fn progress(pool: &PgPool, viewer: &Access, id: i64) -> Result<BulkProgress> {
    let Some(job) = jobs::get(pool, id).await.map_err(Error::from)? else {
        return Err(Error::NotFound("no such job"));
    };
    if job.kind != BulkWork::KIND || job.initiator_id != Some(viewer.user()) {
        return Err(Error::NotFound("no such job"));
    }
    let results = job.payload.get("results").cloned().unwrap_or(Value::Null);
    let failed = results.get("failed").and_then(Value::as_i64).unwrap_or(0);
    let failures = results
        .get("failures")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(BulkProgress {
        id: job.id,
        state: job.state.as_str().to_owned(),
        done: job.progress_done,
        total: job.progress_total,
        failed,
        failures,
        cancel_requested: job.cancel_requested,
        last_error: job.last_error,
    })
}

/// Runs the job to its end: collect the query's IDs in keyset pages — fixed before anything
/// applies, so the apply cannot shift the pages under its own feet — resolve the plan once
/// against their projects (§4.1), then apply per issue, reporting progress between batches.
pub async fn run(pool: &PgPool, ctx: &JobContext, work: BulkWork) -> Result<(), JobError> {
    fn retry<E: std::fmt::Display>(error: E) -> JobError {
        JobError::Retry(error.to_string())
    }
    let actor = access::load(pool, work.actor).await.map_err(retry)?;
    let acting = super::acting_as(
        pool,
        &actor,
        &ApplyInput {
            text: work.text.clone(),
            run_as: work.run_as,
            ..ApplyInput::default()
        },
    )
    .await
    .map_err(retry)?;
    let search = SearchInput {
        text: work.query.clone(),
        project: work.project,
        hide_resolved: work.hide_resolved,
        time_zone: work.time_zone.clone(),
        limit: PAGE,
        ..SearchInput::default()
    };
    let mut ids: Vec<IssueId> = Vec::new();
    let mut cursor = None;
    loop {
        ctx.check()?;
        let mut page_input = search.clone();
        page_input.cursor = cursor;
        let page = search::search(pool, &acting, page_input)
            .await
            .map_err(retry)?;
        ids.extend(page.issues.issues.iter().map(|view| view.issue.id));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    ids.sort_unstable();
    ids.dedup();
    let total = ids.len() as i64;
    // One dictionary over the projects the walk found, one resolution of the line (§4.1).
    let mut conn = pool.acquire().await.map_err(retry)?;
    let found = store_issues::get_many(&mut *conn, &ids)
        .await
        .map_err(retry)?;
    let mut projects: Vec<ProjectId> = found.iter().map(|issue| issue.project_id).collect();
    projects.sort_unstable();
    projects.dedup();
    let mut names = super::dictionary::load_projects(&mut conn, &acting, projects.clone())
        .await
        .map_err(retry)?;
    let input = ApplyInput {
        text: work.text.clone(),
        issues: ids.clone(),
        comment: work.comment.clone(),
        comment_visibility: work.comment_visibility.clone(),
        silent: work.silent,
        create_missing: work.create_missing,
        time_zone: work.time_zone.clone(),
        run_as: None,
    };
    let clock = super::clock(&mut conn, &input).await.map_err(retry)?;
    let mut resolved =
        super::resolve::resolve(&mut conn, &input.text, &names, &acting, &input, &clock)
            .await
            .map_err(retry)?;
    if work.create_missing && resolved.has_errors() {
        let fixes: Vec<_> = resolved
            .problems
            .iter()
            .filter_map(|problem| problem.quick_fix.clone())
            .collect();
        for fix in &fixes {
            let _ = super::apply::create_value(pool, &acting, fix).await;
        }
        names = super::dictionary::load_projects(&mut conn, &acting, projects)
            .await
            .map_err(retry)?;
        resolved = super::resolve::resolve(&mut conn, &input.text, &names, &acting, &input, &clock)
            .await
            .map_err(retry)?;
    }
    if let Some(first) = resolved.problems.first() {
        // Knowable before applying (§4.4): nothing gets the command, the job fails fast.
        return Err(JobError::Fail(format!(
            "the command does not resolve: {:?} over {}..{} {:?}",
            first.code, first.span.start, first.span.end, first.params
        )));
    }
    let operations = resolved.operations;
    let mut done: i64 = 0;
    let mut failed: i64 = 0;
    let mut failures: Vec<Value> = Vec::new();
    for batch in ids.chunks(REPORT) {
        for id in batch {
            ctx.check()?;
            let outcome =
                super::apply::apply_issue(pool, &acting, Origin::Web, *id, &operations, &input)
                    .await;
            if !outcome.ok {
                failed += 1;
                if failures.len() < FAILURES_MAX {
                    let problem = outcome.problem.unwrap_or(super::IssueProblem {
                        code: taskmaster_query::command::CommandCode::CmdWrongKind,
                        detail: String::new(),
                    });
                    failures.push(json!({
                        "issue": outcome.issue,
                        "code": problem.code,
                        "detail": problem.detail,
                    }));
                }
            }
            done += 1;
        }
        ctx.progress(done, Some(total), Some(&format!("{done} of {total}")))
            .await?;
        ctx.save_results(&json!({
            "done": done,
            "total": total,
            "failed": failed,
            "failures": failures,
        }))
        .await?;
    }
    // The history keeps the lines that did something (plan §6): the queued person's, not
    // `run_as` (§4.6).
    if failed < done {
        let mut conn = pool.acquire().await.map_err(retry)?;
        taskmaster_store::command_history::record(&mut conn, work.actor, work.text.trim())
            .await
            .map_err(retry)?;
    }
    Ok(())
}
