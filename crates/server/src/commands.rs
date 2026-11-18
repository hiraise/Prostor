//! The command endpoints (step 2.1 §4.8): applying a command line to named issues, the bulk
//! job over a query and its progress. The line's own failures ride the answers as `cmd_*`
//! codes, never HTTP errors.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::Value;
use taskmaster_api::commands::{
    ApplyAnswer, ApplyRequest, AssistRequest, BulkFailure, BulkProgress, BulkQueued, BulkRequest,
    CommandAssist, CommandProblem, CommandQuickFix, IssueProblem, IssueResult, PlannedOperation,
    Suggestion, SuggestionGroup,
};
use taskmaster_api::error::ErrorBody;
use taskmaster_domain::auth::Scope;
use taskmaster_query::command::CommandCode;
use taskmaster_query::completion;
use taskmaster_query::utf16;
use taskmaster_service::assist as assist_service;
use taskmaster_service::commands::bulk;
use taskmaster_service::commands::{self, ApplyInput, Operation};

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, json};
use crate::issues::origin;
use crate::present::rule_messages;

/// Applies a command line to its issues (CMD-023, plan §4.8): one transaction per issue, each
/// with its own result; plan problems answer every issue and touch nothing.
#[utoipa::path(
    post,
    path = "/commands",
    operation_id = "apply_commands",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = ApplyRequest,
    responses(
        (status = OK, description = "The plan and one result per issue", body = ApplyAnswer),
        (status = BAD_REQUEST, description = "Malformed call", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn apply(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<ApplyRequest>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let input = ApplyInput {
        text: request.text,
        issues: request.issues,
        comment: request.comment,
        comment_visibility: request.comment_visibility.map(audience_input),
        silent: request.silent,
        create_missing: request.create_missing,
        time_zone: request.time_zone,
        run_as: request.run_as,
    };
    let answer =
        commands::apply(&state.db, &viewer.access, origin(&viewer.principal), &input).await?;
    Ok(json(
        StatusCode::OK,
        ApplyAnswer {
            operations: answer
                .plan
                .operations
                .iter()
                .map(|planned| PlannedOperation {
                    kind: kind_of(&planned.operation).to_owned(),
                    start: planned.span.start,
                    end: planned.span.end,
                    names: planned.names.clone(),
                    mode: None,
                    removals: Vec::new(),
                })
                .collect(),
            problems: answer.plan.problems.iter().map(command_problem).collect(),
            issues: answer
                .issues
                .iter()
                .map(|outcome| IssueResult {
                    issue: outcome.issue,
                    ok: outcome.ok,
                    problem: outcome.problem.as_ref().map(|problem| IssueProblem {
                        code: code_of(problem.code),
                        detail: problem.detail.clone(),
                    }),
                    messages: rule_messages(&outcome.messages),
                })
                .collect(),
        },
    ))
}

/// The command dialog's assist (CMD-028, CMD-029, plan §5): the parsed plan for the preview,
/// its problems over ranges with their quick fixes, and the completion at the caret. The line's
/// own failures ride the answer as `cmd_*` codes, never HTTP errors.
#[utoipa::path(
    post,
    path = "/commands/assist",
    operation_id = "assist_commands",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = AssistRequest,
    responses(
        (status = OK, description = "The plan, its problems and the suggestions", body = CommandAssist),
        (status = BAD_REQUEST, description = "No targets named", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn assist(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<AssistRequest>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    // A query of `Some` — even empty — is the bulk flow's target (empty means the named project's
    // every issue, `BulkRequest`); only naming neither target is the mistake.
    if request.issues.is_empty() && request.query.is_none() {
        return Err(ApiError::from(taskmaster_service::Error::field(
            "issues",
            taskmaster_domain::validation::Problem::Required,
            "name the issues or the query the line applies to",
        )));
    }
    let input = assist_service::CommandAssist {
        text: request.text.clone(),
        caret: utf16::from_units(&request.text, request.caret as usize),
        issues: request.issues,
        query: request.query,
        project: request.project,
        hide_resolved: request.hide_resolved,
        comment: request.comment,
        time_zone: request.time_zone,
    };
    let found = assist_service::commands(&state.db, &viewer.access, &input).await?;
    Ok(json(
        StatusCode::OK,
        CommandAssist {
            operations: found
                .plan
                .operations
                .iter()
                .zip(&found.marks)
                .map(|(planned, marks)| PlannedOperation {
                    kind: kind_of(&planned.operation).to_owned(),
                    start: planned.span.start,
                    end: planned.span.end,
                    names: planned.names.clone(),
                    mode: marks.mode.map(str::to_owned),
                    removals: marks.removals.clone(),
                })
                .collect(),
            problems: found.plan.problems.iter().map(command_problem).collect(),
            suggestions: found
                .suggestions
                .iter()
                .map(|found| suggestion(found, &request.text))
                .collect(),
        },
    ))
}

/// Queues a bulk apply over a query (CMD-026, plan §4.7): a job of kind `commands` walks the
/// query's issues and applies the line per issue with progress.
#[utoipa::path(
    post,
    path = "/commands/bulk",
    operation_id = "apply_commands_bulk",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = BulkRequest,
    responses(
        (status = OK, description = "The job was queued", body = BulkQueued),
        (status = BAD_REQUEST, description = "Malformed call", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn apply_bulk(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<BulkRequest>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let queued = bulk::enqueue(
        &state.db,
        &viewer.access,
        bulk::BulkWork {
            actor: viewer.access.user(),
            text: request.text,
            query: request.query,
            project: request.project,
            hide_resolved: request.hide_resolved,
            time_zone: request.time_zone,
            comment: request.comment,
            comment_visibility: request.comment_visibility.map(audience_input),
            silent: request.silent,
            create_missing: request.create_missing,
            run_as: request.run_as,
        },
    )
    .await?;
    Ok(json(StatusCode::OK, BulkQueued { job: queued.id }))
}

/// The bulk job's progress and results (plan §4.7), readable by its owner alone.
#[utoipa::path(
    get,
    path = "/commands/bulk/{id}",
    operation_id = "commands_bulk_progress",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Job ID")),
    responses(
        (status = OK, description = "Progress and per-issue failures", body = BulkProgress),
        (status = NOT_FOUND, description = "No such job of the caller's", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn bulk_progress(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = bulk::progress(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        BulkProgress {
            id: found.id,
            state: found.state,
            done: found.done,
            total: found.total,
            failed: found.failed,
            failures: found
                .failures
                .iter()
                .filter_map(|failure| {
                    let Value::Object(fields) = failure else {
                        return None;
                    };
                    Some(BulkFailure {
                        issue: fields.get("issue")?.as_i64()?,
                        code: fields.get("code")?.as_str()?.to_owned(),
                        detail: fields.get("detail")?.as_str()?.to_owned(),
                    })
                })
                .collect(),
            cancel_requested: found.cancel_requested,
            last_error: found.last_error,
        },
    ))
}

/// The api's audience shape to the service's.
fn audience_input(
    audience: taskmaster_api::issues::AudienceInput,
) -> taskmaster_service::issues::AudienceInput {
    taskmaster_service::issues::AudienceInput {
        users: audience.users,
        groups: audience.groups,
    }
}

fn kind_of(operation: &Operation) -> &'static str {
    match operation {
        Operation::SetField { .. } => "set_field",
        Operation::Tag { .. } => "tag",
        Operation::Move { .. } => "move",
        Operation::Link { .. } => "link",
        Operation::Duplicate { .. } => "duplicate",
        Operation::Visibility { .. } => "visibility",
        Operation::Board { .. } => "board",
        Operation::Star { .. } => "star",
        Operation::Vote { .. } => "vote",
        Operation::Comment => "comment",
        Operation::Delete => "delete",
        Operation::Clone => "clone",
    }
}

/// One problem with its quick fix, in the API's shape.
fn command_problem(problem: &commands::PlanProblem) -> CommandProblem {
    CommandProblem {
        code: code_of(problem.code),
        start: problem.span.start,
        end: problem.span.end,
        params: problem.params.clone(),
        quick_fix: problem.quick_fix.as_ref().map(|fix| CommandQuickFix {
            name: fix.name.clone(),
            field: fix.field,
            projects: fix.projects.clone(),
        }),
    }
}

/// One suggestion in the API's shape: UTF-16 offsets, the ranges into the caller's text.
pub(crate) fn suggestion(found: &completion::Suggestion, text: &str) -> Suggestion {
    let replace = utf16::offsets(text, &[found.replace])[0];
    let matched = utf16::offsets(&found.text, &[found.matched])[0];
    let before: u32 = text[..found.replace.start]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum();
    let added: u32 = found
        .text
        .chars()
        .chain(found.suffix.chars())
        .map(|c| c.len_utf16() as u32)
        .sum();
    Suggestion {
        text: found.text.clone(),
        description: found.description.clone(),
        group: group_of(found.group),
        from: replace.start as u32,
        to: replace.end as u32,
        matched_from: matched.start as u32,
        matched_to: matched.end as u32,
        caret: before + added,
        suffix: found.suffix.clone(),
        present: found.present,
        resolved: found.resolved.clone(),
        own: found.own,
    }
}

fn group_of(group: completion::Group) -> SuggestionGroup {
    use completion::Group as G;
    match group {
        G::Command => SuggestionGroup::Command,
        G::Attribute => SuggestionGroup::Attribute,
        G::Value => SuggestionGroup::Value,
        G::Link => SuggestionGroup::Link,
        G::Tag => SuggestionGroup::Tag,
        G::Project => SuggestionGroup::Project,
        G::User => SuggestionGroup::User,
        G::Group => SuggestionGroup::Group,
        G::Board => SuggestionGroup::Board,
        G::Sprint => SuggestionGroup::Sprint,
        G::Special => SuggestionGroup::Special,
        G::Empty => SuggestionGroup::Empty,
        G::Date => SuggestionGroup::Date,
        G::Issue => SuggestionGroup::Issue,
        G::Recent => SuggestionGroup::Recent,
        G::Keyword => SuggestionGroup::Keyword,
        G::SortKey => SuggestionGroup::SortKey,
        G::SortDirection => SuggestionGroup::SortDirection,
    }
}

fn code_of(code: CommandCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "cmd_unknown_word".to_owned())
}
