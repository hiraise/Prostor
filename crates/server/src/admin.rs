//! The general admin screens over HTTP (plan step 1.11 §3): the settings of «Настройки» and the
//! jobs of «Задания» (ADM-031, ADM-032). Reading needs `system.read`, acting `system.write`.
//! The mail settings keep their own screen and API (`admin_mail`, 1.10c).

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use taskmaster_api::admin::{
    AdminFlag, AdminJob, AdminJobState, AdminJobs, AdminJobsQuery, AdminSetting, AdminSettings,
    AdminSettingsChange, JobProgress,
};
use taskmaster_api::error::ErrorBody;
use taskmaster_domain::UserId;
use taskmaster_domain::access::Access;
use taskmaster_domain::auth::Scope;
use taskmaster_store::jobs::{Job, JobFilter, JobState, Term};
use taskmaster_store::users::{self as store_users, User};

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::present::user_ref;

/// How many jobs one page carries at most (ADM-031); a lower maximum than the API's 200.
const JOBS_MAX_LIMIT: u32 = 200;
/// Jobs per page when the client does not ask for a number.
const JOBS_DEFAULT_LIMIT: u32 = 50;

/// The settings of the «Настройки» screen: value, default and `is_stored` per key, the flags,
/// and the deployment's upload ceiling. The values come from the node's snapshot: current
/// without touching the database.
#[utoipa::path(
    get,
    path = "/admin/settings",
    operation_id = "get_admin_settings",
    tag = "admin",
    security(("session" = []), ("token" = [])),
    responses(
        (status = OK, description = "The general settings", body = AdminSettings),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.read`", body = ErrorBody),
    ),
)]
pub async fn settings(State(state): State<AppState>, viewer: Viewer) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let view = taskmaster_service::settings::admin_view(&viewer.access, &state.settings.get())?;
    Ok(json(StatusCode::OK, shape(&view, state.upload_ceiling)))
}

/// Saves a change set of the general settings (ADM-002): each present key changes, `null`
/// returns it to the default. One transaction, one audit event per key; every node reloads.
/// The mail settings refuse this API — they have their own.
#[utoipa::path(
    put,
    path = "/admin/settings",
    operation_id = "save_admin_settings",
    tag = "admin",
    security(("session" = []), ("token" = [])),
    request_body = AdminSettingsChange,
    responses(
        (status = OK, description = "Saved; the settings as they stand now", body = AdminSettings),
        (status = BAD_REQUEST, description = "An unknown key or an invalid value", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`, or a mail key", body = ErrorBody),
    ),
)]
pub async fn save(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<AdminSettingsChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let changes: Vec<(String, Option<serde_json::Value>)> = request.0.into_iter().collect();
    taskmaster_service::settings::save(
        &state.db,
        &viewer.access,
        &changes,
        state.settings.secret_key(),
    )
    .await?;
    // This node follows at once; other nodes follow the settings bus.
    state.settings.reload(&state.db).await?;
    let view = taskmaster_service::settings::admin_view(&viewer.access, &state.settings.get())?;
    Ok(json(StatusCode::OK, shape(&view, state.upload_ceiling)))
}

/// The jobs of «Задания» (ADM-031): newest first, keyset-paginated, with a state filter and
/// words matched against the kind and the initiator's name (ADM-032).
#[utoipa::path(
    get,
    path = "/admin/jobs",
    operation_id = "list_admin_jobs",
    tag = "admin",
    security(("session" = []), ("token" = [])),
    params(AdminJobsQuery),
    responses(
        (status = OK, description = "A page of jobs, newest first", body = AdminJobs),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.read`", body = ErrorBody),
    ),
)]
pub async fn jobs(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<AdminJobsQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let limit = query
        .limit
        .unwrap_or(JOBS_DEFAULT_LIMIT)
        .clamp(1, JOBS_MAX_LIMIT) as i64;
    let filter = JobFilter {
        initiator: None,
        state: query.state.map(|state| match state {
            AdminJobState::Queued => JobState::Queued,
            AdminJobState::Running => JobState::Running,
            AdminJobState::Finished => JobState::Finished,
            AdminJobState::Failed => JobState::Failed,
            AdminJobState::Cancelled => JobState::Cancelled,
        }),
        terms: Term::parse(query.q.as_deref().unwrap_or("")),
    };
    // One row more than the page tells whether another page follows.
    let rows =
        taskmaster_service::jobs::list(&state.db, &viewer.access, filter, query.before, limit + 1)
            .await?;
    let more = rows.len() as i64 > limit;
    let rows: Vec<Job> = rows.into_iter().take(limit as usize).collect();
    let next = more.then(|| rows.last().map(|job| job.id)).flatten();
    let users = initiators(&state, &rows).await?;
    let names = state.names();
    let items = rows
        .iter()
        .map(|job| present_job(&names, &viewer.access, &users, job))
        .collect();
    Ok(json(StatusCode::OK, AdminJobs { items, next }))
}

/// Cancels a job (ADM-031): a queued one at once, a running one only when its kind may be
/// cancelled; anything already final or not cancellable answers 409.
#[utoipa::path(
    post,
    path = "/admin/jobs/{id}/cancel",
    operation_id = "cancel_admin_job",
    tag = "admin",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "The job")),
    responses(
        (status = OK, description = "Cancelled, or the stop was requested; the job as it stands", body = AdminJob),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such job", body = ErrorBody),
        (status = CONFLICT, description = "Not cancellable, or already final", body = ErrorBody),
    ),
)]
pub async fn cancel(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let job = taskmaster_service::jobs::cancel(&state.db, &viewer.access, id).await?;
    job_answer(&state, &viewer, job).await
}

/// Retries a failed job (ADM-032): a fresh one with the same kind and payload, the administrator
/// as its initiator. Anything not failed answers 409.
#[utoipa::path(
    post,
    path = "/admin/jobs/{id}/retry",
    operation_id = "retry_admin_job",
    tag = "admin",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "The failed job")),
    responses(
        (status = CREATED, description = "The fresh job, queued", body = AdminJob),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such job", body = ErrorBody),
        (status = CONFLICT, description = "Only a failed job can be retried", body = ErrorBody),
    ),
)]
pub async fn retry(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let job = taskmaster_service::jobs::retry(&state.db, &viewer.access, id).await?;
    let mut response = job_answer(&state, &viewer, job).await?;
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

/// The users who started these jobs, for their `UserRef`s — one query, not one per row.
async fn initiators(state: &AppState, jobs: &[Job]) -> Result<HashMap<UserId, User>, ApiError> {
    let mut ids: Vec<UserId> = jobs.iter().filter_map(|job| job.initiator_id).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(store_users::get_many(&state.db, &ids)
        .await?
        .into_iter()
        .map(|user| (user.id, user))
        .collect())
}

/// A job with its initiator's ref, as a 200 response.
async fn job_answer(state: &AppState, viewer: &Viewer, job: Job) -> Result<Response, ApiError> {
    let users = initiators(state, std::slice::from_ref(&job)).await?;
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        present_job(&names, &viewer.access, &users, &job),
    ))
}

/// A job as the API shape (ADM-031).
fn present_job(
    names: &taskmaster_service::people::Names,
    viewer: &Access,
    users: &HashMap<UserId, User>,
    job: &Job,
) -> AdminJob {
    AdminJob {
        id: job.id,
        kind: job.kind.clone(),
        state: match job.state {
            JobState::Queued => AdminJobState::Queued,
            JobState::Running => AdminJobState::Running,
            JobState::Finished => AdminJobState::Finished,
            JobState::Failed => AdminJobState::Failed,
            JobState::Cancelled => AdminJobState::Cancelled,
        },
        initiator: job
            .initiator_id
            .and_then(|id| users.get(&id))
            .map(|user| user_ref(names, viewer, user)),
        cancellable: job.cancellable,
        cancel_requested: job.cancel_requested,
        attempts: job.attempts,
        max_attempts: job.max_attempts,
        progress: JobProgress {
            done: job.progress_done,
            total: job.progress_total,
        },
        status_text: job.status_text.clone(),
        last_error: job.last_error.clone(),
        created_at: job.created_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
    }
}

/// The service view as the API shape.
fn shape(
    view: &taskmaster_service::settings::AdminSettingsView,
    upload_ceiling: u64,
) -> AdminSettings {
    AdminSettings {
        settings: view
            .settings
            .iter()
            .map(|setting| AdminSetting {
                key: setting.key.clone(),
                value: setting.value.clone(),
                default: setting.default.clone(),
                is_stored: setting.is_stored,
            })
            .collect(),
        flags: view
            .flags
            .iter()
            .map(|flag| AdminFlag {
                key: flag.key.clone(),
                description: flag.description.clone(),
                value: flag.value,
                default: flag.default,
                is_stored: flag.is_stored,
                may_switch_off: flag.may_switch_off,
            })
            .collect(),
        upload_ceiling,
    }
}
