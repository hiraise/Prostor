//! `/issues` and `/tags`: issues and drafts, their fields, audience, moves, stars, votes, tags and
//! activity stream (ISS, HIST). Rules and flows live in `trackflow_service::issues`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::Value as Json;
use trackflow_api::error::ErrorBody;
use trackflow_api::fields::{FieldRef, IssueImpact};
use trackflow_api::issues::{
    ActivityDirection, ActivityItem, ActivityPage, ActivityQuery, Audience, AudienceCandidates,
    CandidateQuery, ChangeValues, CloneDraftsAnswer, CloneDraftsRequest, CloneOfRef, CommentItem,
    FieldOptions, FieldOptionsQuery, FieldValueChange, FieldValueInput, FieldValuesChange,
    HeldValue, HistoryItem, HistoryKind, HistoryOrigin, Issue, IssueAbilities, IssueAnswer,
    IssueBatch, IssueBatchRequest, IssueField, IssuePage, IssuePatch, IssuePlace, LoggedBoard,
    LoggedFile, LoggedValue, MentionCandidates, MentionQuery, MoveChange, MoveIssue, MoveOutcome,
    MovePreview, MovePreviewQuery, MoveResult, NewIssue, NewTag, OptionValue, Tag, TagPatch,
    TagQuery, TagRef, TagSharing,
};
use trackflow_api::links::{LoggedLink as ApiLoggedLink, SubtaskCount};
use trackflow_api::page::{Page, PageQuery};
use trackflow_domain::access::Access;
use trackflow_domain::auth::{Credential, Principal, Scope};
use trackflow_domain::fields::FieldKind;
use trackflow_domain::issues::history::{
    ChangeKind, LoggedLink, Origin, clone_of as clone_source, logged, read_ref,
};
use trackflow_domain::issues::moves::Outcome;
use trackflow_domain::issues::values::{Input, Value};
use trackflow_domain::{FieldId, IssueId, TagId};
use trackflow_service::files::Signer;
use trackflow_service::issue_links::{NewLinks, side_name};
use trackflow_service::issues::activity::{self, ActivityPage as ServiceActivity, Item};
use trackflow_service::issues::{
    self, AudienceInput, Described, FieldChange, FieldInput, IssueChange, IssueView,
};
use trackflow_service::people::Names;
use trackflow_service::tags;
use trackflow_store::issue_history::Direction;
use trackflow_store::tags::Tag as StoredTag;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::comments::comment_from;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::fields::field_ref;
use crate::issue_links::{domain_side, issue_brief, link_type_ref, side};
use crate::present::{group_ref, project_ref, rule_messages, text_refs, user_ref};
use crate::sharing::{Shown, shared_with, tag_listing};

const TAGS_MAX_LIMIT: u32 = 200;
const HISTORY_DEFAULT_LIMIT: i64 = 50;

/// Where a change comes from: the web app's session or a token (HIST-012).
pub(crate) fn origin(principal: &Principal) -> Origin {
    match principal.credential {
        Credential::Session { .. } => Origin::Web,
        Credential::Token { .. } => Origin::Api,
    }
}

fn input(value: FieldValueInput) -> Input {
    match value {
        FieldValueInput::Integer(n) => Input::Integer(n),
        FieldValueInput::Float(n) => Input::Float(n),
        FieldValueInput::Text(t) => Input::Text(t),
        FieldValueInput::Ids(ids) => Input::Ids(ids),
    }
}

pub(crate) fn audience_input(a: trackflow_api::issues::AudienceInput) -> AudienceInput {
    AudienceInput {
        users: a.users,
        groups: a.groups,
    }
}

pub fn tag_ref(tag: &StoredTag) -> TagRef {
    TagRef {
        id: tag.id,
        name: tag.name.clone(),
        color: tag.color,
    }
}

/// Tags with their owners, audiences and the caller's abilities: one load for the whole page.
async fn tags_shown(
    state: &AppState,
    viewer: &Access,
    rows: &[StoredTag],
) -> Result<Vec<Tag>, ApiError> {
    let shared: Vec<_> = rows.iter().map(StoredTag::sharing).collect();
    let parties = tags::parties(&state.db, &shared).await?;
    let shown = Shown::new(state.names(), viewer, parties);
    rows.iter()
        .zip(&shared)
        .map(|(tag, s)| {
            Ok(Tag {
                id: tag.id,
                name: tag.name.clone(),
                color: tag.color,
                owner: shown.owner(s)?,
                sharing: shown.sees_sharing(s).then(|| TagSharing {
                    view: shown.audience(&s.view),
                    use_: shown.audience(&s.use_),
                    edit: shown.audience(&s.edit),
                }),
                abilities: shown.tag_abilities(s),
                version: tag.version,
                created_at: tag.created_at,
            })
        })
        .collect()
}

async fn tag_shown(state: &AppState, viewer: &Access, row: StoredTag) -> Result<Tag, ApiError> {
    tags_shown(state, viewer, std::slice::from_ref(&row))
        .await?
        .pop()
        .ok_or_else(|| trackflow_service::Error::Internal("the tag is missing".to_owned()).into())
}

fn issue_field(names: &Names, viewer: &Access, d: &Described, f: &issues::FieldView) -> IssueField {
    let mut out = IssueField {
        field: field_ref(&f.field.field),
        empty_label: f.field.attached.empty_label.clone(),
        private: f.field.private(),
        required: f.required,
        editable: f.editable,
        text: None,
        html: f.rendered.as_ref().map(|r| r.html.clone()),
        integer: None,
        float: None,
        date: None,
        date_time: None,
        minutes: None,
        values: Vec::new(),
        users: Vec::new(),
        groups: Vec::new(),
    };
    match &f.value {
        None => {}
        Some(Value::String(t) | Value::Text(t)) => out.text = Some(t.clone()),
        Some(Value::Integer(n)) => out.integer = Some(*n),
        Some(Value::Float(n)) => out.float = Some(*n),
        Some(Value::Date(day)) => out.date = Some(*day),
        Some(Value::DateTime(at)) => out.date_time = Some(*at),
        Some(Value::Duration(n)) => out.minutes = Some(*n),
        Some(Value::Refs(ids)) => match f.field.kind.kind {
            FieldKind::User => {
                out.users = ids
                    .iter()
                    .filter_map(|id| d.user(*id))
                    .map(|u| user_ref(names, viewer, u))
                    .collect();
            }
            FieldKind::Group => {
                out.groups = ids
                    .iter()
                    .filter_map(|id| d.group(*id))
                    .map(group_ref)
                    .collect();
            }
            _ => {
                let mut held: Vec<_> = ids.iter().filter_map(|id| d.value(*id)).collect();
                held.sort_by(|a, b| a.rank.cmp(&b.rank));
                out.values = held.into_iter().map(held_value).collect();
            }
        },
    }
    out
}

pub(crate) fn issue(
    names: &Names,
    signer: &Signer,
    viewer: &Access,
    d: &Described,
    view: &IssueView,
) -> Issue {
    let i = &view.issue;
    let person = |id| d.user(id).map(|u| user_ref(names, viewer, u));
    let mut mentioned = i.mentioned_users.clone();
    let mut tokens = i.issue_tokens.clone();
    let mut files = i.file_names.clone();
    for r in view.fields.iter().filter_map(|f| f.rendered.as_ref()) {
        mentioned.extend(&r.users);
        tokens.extend(r.issue_tokens.iter().cloned());
        files.extend(r.files.iter().cloned());
    }
    Issue {
        id: i.id,
        readable_id: view.readable_id(),
        number: i.number,
        draft: i.is_draft(),
        project: project_ref(&view.project),
        summary: i.summary.clone(),
        description: i.description.clone(),
        description_html: i.description_html.clone(),
        refs: text_refs(names, viewer, signer, &d.refs, &mentioned, &tokens, &files),
        reporter: person(i.reporter_id).unwrap_or_else(|| placeholder(i.reporter_id)),
        created_at: i.created_at,
        updated_at: i.updated_at,
        updater: i.updated_by.and_then(person),
        resolved_at: i.resolved_at,
        audience: Audience {
            users: i
                .audience_users
                .iter()
                .filter_map(|id| person(*id))
                .collect(),
            groups: i
                .audience_groups
                .iter()
                .filter_map(|id| d.group(*id))
                .map(group_ref)
                .collect(),
        },
        fields: view
            .fields
            .iter()
            .map(|f| issue_field(names, viewer, d, f))
            .collect(),
        tags: view.tags.iter().map(tag_ref).collect(),
        starred: view.starred,
        voted: view.voted,
        votes: i.votes,
        watchers: view.watchers,
        can: IssueAbilities {
            update: view.can.update,
            update_private: view.can.update_private,
            delete: view.can.delete,
            manage_watchers: view.can.manage_watchers,
            vote: view.can.vote,
            silent: view.can.silent,
        },
        subtasks: SubtaskCount {
            total: view.subtasks.total,
            unresolved: view.subtasks.unresolved,
        },
        parent: view
            .parent
            .as_ref()
            .and_then(|(l, p)| issue_brief(l, std::slice::from_ref(p))),
        version: i.version,
        text_version: i.text_version,
    }
}

/// A stand-in for an account that is not loaded; never expected, but answers stay well-formed.
pub(crate) fn placeholder(id: i64) -> trackflow_api::accounts::UserRef {
    trackflow_api::accounts::UserRef {
        id,
        username: "deleted".to_owned(),
        full_name: "Deleted user".to_owned(),
        avatar_color: 0,
        avatar_url: None,
        kind: trackflow_api::accounts::UserRefKind::Deleted,
        pseudonym: None,
        disabled: true,
    }
}

fn one(state: &AppState, viewer: &Access, (view, d): &issues::One) -> Issue {
    issue(&state.names(), &state.signer(), viewer, d, view)
}

/// One issue in full, by internal ID or readable ID, current or former (ISS-001…ISS-003). The
/// answer's `readable_id` is the current one: when it differs from the one asked for, the issue
/// moved or its project's key changed.
#[utoipa::path(
    get,
    path = "/issues/{id}",
    operation_id = "get_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = String, Path, description = "Internal ID or readable ID such as `WEB-12`")),
    responses(
        (status = OK, description = "The issue", body = Issue),
        (status = NOT_FOUND, description = "No such issue, or the caller may not see it", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let id = match id.parse::<IssueId>() {
        Ok(id) => id,
        Err(_) => issues::resolve(&state.db, &viewer.access, &id).await?.0,
    };
    let found = issues::get(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, one(&state, &viewer.access, &found)))
}

/// The caller's drafts, newest first (ISS-002).
#[utoipa::path(
    get,
    path = "/issues/drafts",
    operation_id = "list_drafts",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    responses((status = OK, description = "Drafts", body = Page<Issue>)),
)]
pub async fn drafts(State(state): State<AppState>, viewer: Viewer) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let d = issues::drafts(&state.db, &viewer.access).await?;
    let names = state.names();
    let signer = state.signer();
    Ok(json(
        StatusCode::OK,
        Page {
            items: d
                .issues
                .iter()
                .map(|v| issue(&names, &signer, &viewer.access, &d, v))
                .collect(),
            next_cursor: None,
        },
    ))
}

/// Deletes all of the caller's drafts with their files (ISS-011).
#[utoipa::path(
    delete,
    path = "/issues/drafts",
    operation_id = "delete_drafts",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    responses((status = NO_CONTENT, description = "Deleted")),
)]
pub async fn delete_drafts(
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    issues::delete_drafts(&state.db, &viewer.access).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Several issues by internal ID (ISS-001): readable published issues and the caller's drafts, in
/// the order asked; missing ones are left out.
#[utoipa::path(
    post,
    path = "/issues/batch",
    operation_id = "get_issues",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = IssueBatchRequest,
    responses(
        (status = OK, description = "The issues found", body = IssueBatch),
        (status = BAD_REQUEST, description = "No IDs or more than 100", body = ErrorBody),
    ),
)]
pub async fn batch(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<IssueBatchRequest>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let d = issues::describe_many(&state.db, &viewer.access, &request.ids).await?;
    let names = state.names();
    let signer = state.signer();
    Ok(json(
        StatusCode::OK,
        IssueBatch {
            items: d
                .issues
                .iter()
                .map(|v| issue(&names, &signer, &viewer.access, &d, v))
                .collect(),
        },
    ))
}

/// What a field of an issue may hold for the caller (FLD-001…FLD-014), filtered by `q`.
#[utoipa::path(
    get,
    path = "/issues/{id}/fields/{field_id}/options",
    operation_id = "field_options",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("field_id" = i64, Path, description = "Field ID"),
        FieldOptionsQuery,
    ),
    responses(
        (status = OK, description = "Options", body = FieldOptions),
        (status = BAD_REQUEST, description = "Invalid limit", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or field", body = ErrorBody),
    ),
)]
pub async fn field_options(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field)): Path<(IssueId, FieldId)>,
    QueryParams(query): QueryParams<FieldOptionsQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let max = issues::options::LIMIT_MAX;
    let limit = match query.limit {
        None => 20,
        Some(limit) if (1..=max).contains(&i64::from(limit)) => i64::from(limit),
        Some(_) => {
            return Err(ApiError::invalid(vec![
                trackflow_api::error::FieldError::new(
                    "limit",
                    "out_of_range",
                    format!("limit must be between 1 and {max}"),
                ),
            ]));
        }
    };
    let found = issues::options::field_options(
        &state.db,
        &viewer.access,
        id,
        field,
        query.q.as_deref().unwrap_or_default(),
        limit,
    )
    .await?;
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        FieldOptions {
            values: found.values.iter().map(option_value).collect(),
            users: found
                .users
                .iter()
                .map(|u| user_ref(&names, &viewer.access, u))
                .collect(),
            groups: found.groups.iter().map(group_ref).collect(),
            more: found.more,
            can_add_value: found.can_add_value,
            set: found.set,
        },
    ))
}

pub(crate) fn held_value(v: &trackflow_store::value_sets::Value) -> HeldValue {
    HeldValue {
        id: v.id,
        name: v.name.clone(),
        color: v.color,
        description: v.description.clone(),
        archived: v.archived,
        resolved: v.resolved,
    }
}

fn option_value(v: &trackflow_store::value_sets::Value) -> OptionValue {
    OptionValue {
        id: v.id,
        name: v.name.clone(),
        color: v.color,
        description: v.description.clone(),
        archived: v.archived,
        resolved: v.resolved,
        released: v.released,
        release_date: v.release_date,
    }
}

/// Creates an issue, or a draft with `draft: true` (ISS-002, ISS-005…ISS-007).
#[utoipa::path(
    post,
    path = "/issues",
    operation_id = "create_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = NewIssue,
    responses(
        (status = CREATED, description = "Created", body = IssueAnswer),
        (status = BAD_REQUEST, description = "Invalid values or missing required fields", body = ErrorBody),
        (status = FORBIDDEN, description = "No `issue.create`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewIssue>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let created = issues::create(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        issues::NewIssue {
            project: request.project,
            summary: request.summary,
            description: request.description,
            fields: request
                .fields
                .into_iter()
                .map(|f| FieldInput {
                    field: f.field,
                    value: f.value.map(input),
                })
                .collect(),
            audience: request.audience.map(audience_input),
            tags: request.tags,
            watchers: request.watchers,
            links: request
                .links
                .into_iter()
                .map(|l| NewLinks {
                    link_type: l.link_type,
                    side: domain_side(l.side),
                    issues: vec![l.issue],
                })
                .collect(),
            publish: !request.draft,
            uploads: request.uploads,
        },
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        IssueAnswer {
            issue: one(&state, &viewer.access, &created.done),
            messages: rule_messages(&created.messages),
        },
    ))
}

/// Publishes a draft (ISS-002): it gets the project's next number.
#[utoipa::path(
    post,
    path = "/issues/{id}/publish",
    operation_id = "publish_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Draft ID")),
    responses(
        (status = OK, description = "Published", body = IssueAnswer),
        (status = BAD_REQUEST, description = "No summary or missing required fields", body = ErrorBody),
        (status = NOT_FOUND, description = "No such draft", body = ErrorBody),
        (status = CONFLICT, description = "Published already, or the project is archived", body = ErrorBody),
    ),
)]
pub async fn publish(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let done = issues::publish(&state.db, &viewer.access, origin(&viewer.principal), id).await?;
    Ok(json(
        StatusCode::OK,
        IssueAnswer {
            issue: one(&state, &viewer.access, &done.done),
            messages: rule_messages(&done.messages),
        },
    ))
}

/// Clones an issue into a published copy of the same project (AUT-066): the summary gains a
/// «(copy)» suffix, every field value, the audience and the caller's own tags come along;
/// comments, files, links, votes, watchers and history do not. Needs read of the source,
/// `issue.create` there and the project's `clone_issue` toggle on.
#[utoipa::path(
    post,
    path = "/issues/{id}/clone",
    operation_id = "clone_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = CREATED, description = "The copy, published", body = IssueAnswer),
        (status = BAD_REQUEST, description = "The summary with its suffix is too long", body = ErrorBody),
        (status = FORBIDDEN, description = "No create right, or cloning is off in the project", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn clone(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let done = trackflow_service::rules::clone::clone_issue(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        IssueAnswer {
            issue: one(&state, &viewer.access, &done.done),
            messages: rule_messages(&done.messages),
        },
    ))
}

/// Clones issues into the caller's drafts (AUT-066): the same copies without numbers, ready in
/// the drafts menu. Sources the caller cannot read are skipped; a readable source whose project
/// refuses cloning refuses the whole request.
#[utoipa::path(
    post,
    path = "/issues/clone-drafts",
    operation_id = "clone_drafts",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = CloneDraftsRequest,
    responses(
        (status = OK, description = "The drafts, in the order asked", body = CloneDraftsAnswer),
        (status = BAD_REQUEST, description = "No IDs or more than 50", body = ErrorBody),
        (status = FORBIDDEN, description = "No create right, or cloning is off in a source's project", body = ErrorBody),
    ),
)]
pub async fn clone_drafts(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<CloneDraftsRequest>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let done =
        trackflow_service::rules::clone::clone_drafts(&state.db, &viewer.access, &request.issues)
            .await?;
    let names = state.names();
    let signer = state.signer();
    Ok(json(
        StatusCode::OK,
        CloneDraftsAnswer {
            drafts: done
                .done
                .issues
                .iter()
                .map(|v| issue(&names, &signer, &viewer.access, &done.done, v))
                .collect(),
            messages: rule_messages(&done.messages),
        },
    ))
}

/// Changes summary, description and audience (ISS-018, ISS-041). Text edits carry the
/// `text_version` they started from: if someone changed the texts meanwhile, the answer is
/// `conflict` and the client shows both versions (ISS-051).
#[utoipa::path(
    patch,
    path = "/issues/{id}",
    operation_id = "update_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    request_body = IssuePatch,
    responses(
        (status = OK, description = "The issue now", body = IssueAnswer),
        (status = BAD_REQUEST, description = "Invalid values", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
        (status = CONFLICT, description = "The texts changed meanwhile, or the project is archived", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
    JsonBody(patch): JsonBody<IssuePatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let done = issues::update(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        IssueChange {
            summary: patch.summary,
            description: patch.description,
            text_version: patch.text_version,
            audience: patch.audience.map(audience_input),
            silent: patch.silent,
        },
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        IssueAnswer {
            issue: one(&state, &viewer.access, &done.done),
            messages: rule_messages(&done.messages),
        },
    ))
}

async fn change_field(
    state: &AppState,
    viewer: &Viewer,
    id: IssueId,
    field: i64,
    change: FieldChange,
    silent: bool,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let done = issues::set_field(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        field,
        change,
        silent,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        IssueAnswer {
            issue: one(state, &viewer.access, &done.done),
            messages: rule_messages(&done.messages),
        },
    ))
}

/// Sets one field's value at once (ISS-019); `null` clears it.
#[utoipa::path(
    put,
    path = "/issues/{id}/fields/{field_id}",
    operation_id = "set_issue_field",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("field_id" = i64, Path, description = "Field ID"),
    ),
    request_body = FieldValueChange,
    responses(
        (status = OK, description = "The issue now", body = IssueAnswer),
        (status = BAD_REQUEST, description = "Not a value of the field, or it cannot be empty", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or field", body = ErrorBody),
        (status = CONFLICT, description = "A computed field, or the project is archived", body = ErrorBody),
    ),
)]
pub async fn set_field(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field)): Path<(IssueId, i64)>,
    JsonBody(change): JsonBody<FieldValueChange>,
) -> Result<Response, ApiError> {
    change_field(
        &state,
        &viewer,
        id,
        field,
        FieldChange::Set(change.value.map(input)),
        change.silent,
    )
    .await
}

/// Adds values to a multi-valued field (FLD-001).
#[utoipa::path(
    post,
    path = "/issues/{id}/fields/{field_id}/add",
    operation_id = "add_issue_field_values",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("field_id" = i64, Path, description = "Field ID"),
    ),
    request_body = FieldValuesChange,
    responses(
        (status = OK, description = "The issue now", body = IssueAnswer),
        (status = BAD_REQUEST, description = "Not values of the field", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or field", body = ErrorBody),
    ),
)]
pub async fn add_values(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field)): Path<(IssueId, i64)>,
    JsonBody(change): JsonBody<FieldValuesChange>,
) -> Result<Response, ApiError> {
    change_field(
        &state,
        &viewer,
        id,
        field,
        FieldChange::Add(change.ids),
        change.silent,
    )
    .await
}

/// Removes values from a multi-valued field (FLD-001).
#[utoipa::path(
    post,
    path = "/issues/{id}/fields/{field_id}/remove",
    operation_id = "remove_issue_field_values",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("field_id" = i64, Path, description = "Field ID"),
    ),
    request_body = FieldValuesChange,
    responses(
        (status = OK, description = "The issue now", body = IssueAnswer),
        (status = BAD_REQUEST, description = "The field cannot be empty", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or field", body = ErrorBody),
    ),
)]
pub async fn remove_values(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field)): Path<(IssueId, i64)>,
    JsonBody(change): JsonBody<FieldValuesChange>,
) -> Result<Response, ApiError> {
    change_field(
        &state,
        &viewer,
        id,
        field,
        FieldChange::Remove(change.ids),
        change.silent,
    )
    .await
}

/// Deletes an issue (ISS-028): it leaves every list and stays whole for the trash. A draft goes
/// for good.
#[utoipa::path(
    delete,
    path = "/issues/{id}",
    operation_id = "delete_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "No `issue.delete`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    issues::delete(&state.db, &viewer.access, origin(&viewer.principal), id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn values_of(
    names: &Names,
    viewer: &Access,
    kind: Option<FieldKind>,
    value: Option<&Value>,
    d: &Described,
) -> ChangeValues {
    let items = logged(value, &|id| d.value(id).map(|v| v.name.clone()));
    change_values(names, viewer, kind, &Json::Array(items), &Lookups::Issue(d))
}

/// What history answers can name: an issue's lookups or an activity page's.
enum Lookups<'a> {
    Issue(&'a Described),
    History(&'a ServiceActivity),
}

impl Lookups<'_> {
    fn user(&self, id: i64) -> Option<&trackflow_store::users::User> {
        match self {
            Lookups::Issue(d) => d.user(id),
            Lookups::History(h) => h.users.iter().find(|u| u.id == id),
        }
    }

    fn group(&self, id: i64) -> Option<&trackflow_store::groups::Group> {
        match self {
            Lookups::Issue(d) => d.group(id),
            Lookups::History(h) => h.groups.iter().find(|g| g.id == id),
        }
    }

    fn value(&self, id: i64) -> Option<&trackflow_store::value_sets::Value> {
        match self {
            Lookups::Issue(d) => d.value(id),
            Lookups::History(h) => h.values.iter().find(|v| v.id == id),
        }
    }
}

/// Reads logged field values (`crates/domain/src/issues/history.rs`) for a field of a kind.
fn change_values(
    names: &Names,
    viewer: &Access,
    kind: Option<FieldKind>,
    json: &Json,
    look: &Lookups<'_>,
) -> ChangeValues {
    let mut out = ChangeValues::default();
    let items = json.as_array().cloned().unwrap_or_default();
    for item in &items {
        match kind {
            Some(FieldKind::User) => {
                if let Some(id) = item.as_i64() {
                    out.users.push(match look.user(id) {
                        Some(u) => user_ref(names, viewer, u),
                        None => placeholder(id),
                    });
                }
            }
            Some(FieldKind::Group) => {
                if let Some(g) = item.as_i64().and_then(|id| look.group(id)) {
                    out.groups.push(group_ref(g));
                }
            }
            Some(k) if k.set_kind().is_some() => {
                if let Some((id, name)) = read_ref(item) {
                    let current = look.value(id);
                    out.values.push(LoggedValue {
                        id,
                        name: name
                            .or_else(|| current.map(|v| v.name.clone()))
                            .unwrap_or_default(),
                        color: current.and_then(|v| v.color),
                        deleted: current.is_none(),
                    });
                }
            }
            _ => match item {
                Json::String(text) => out.texts.push(text.clone()),
                Json::Number(n) => out.numbers.extend(n.as_f64()),
                _ => {}
            },
        }
    }
    out
}

/// What moving an issue to another project would do (ISS-025).
#[utoipa::path(
    get,
    path = "/issues/{id}/move-preview",
    operation_id = "preview_issue_move",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID"), MovePreviewQuery),
    responses(
        (status = OK, description = "What would change", body = MovePreview),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or project", body = ErrorBody),
        (status = CONFLICT, description = "The same project, or a project is archived", body = ErrorBody),
    ),
)]
pub async fn move_preview(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
    QueryParams(query): QueryParams<MovePreviewQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let preview = issues::moves::preview(&state.db, &viewer.access, id, query.project).await?;
    // Names of the values involved.
    let (_, d) = issues::get(&state.db, &viewer.access, id).await?;
    let names = state.names();
    let changes = preview
        .changes
        .iter()
        .map(|(f, before, outcome)| {
            let kind = Some(f.kind.kind);
            let (outcome, after) = match outcome {
                Outcome::Erased | Outcome::Kept => (MoveOutcome::Erased, None),
                Outcome::Replaced(after) => (MoveOutcome::Replaced, after.as_ref()),
            };
            MoveChange {
                field: field_ref(&f.field),
                outcome,
                before: values_of(&names, &viewer.access, kind, before.as_ref(), &d),
                after: values_of(&names, &viewer.access, kind, after, &d),
            }
        })
        .collect();
    Ok(json(
        StatusCode::OK,
        MovePreview {
            target: project_ref(&preview.target),
            changes,
            loses_access: preview.loses_access,
        },
    ))
}

/// Moves an issue to another project (ISS-025): the target's next number; the old ID keeps
/// leading to it.
#[utoipa::path(
    post,
    path = "/issues/{id}/move",
    operation_id = "move_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    request_body = MoveIssue,
    responses(
        (status = OK, description = "Moved", body = MoveResult),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or project", body = ErrorBody),
        (status = CONFLICT, description = "The same project, or a project is archived", body = ErrorBody),
    ),
)]
pub async fn move_issue(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
    JsonBody(request): JsonBody<MoveIssue>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let moved = issues::moves::move_issue(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        request.project,
        request.silent,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        MoveResult {
            issue: moved.done.map(|found| one(&state, &viewer.access, &found)),
            messages: rule_messages(&moved.messages),
        },
    ))
}

async fn star(
    state: &AppState,
    viewer: &Viewer,
    id: IssueId,
    on: bool,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    issues::set_star(&state.db, &viewer.access, id, on).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Stars the issue: the caller watches it (ISS-030).
#[utoipa::path(
    put,
    path = "/issues/{id}/star",
    operation_id = "star_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = NO_CONTENT, description = "Starred"),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn add_star(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    star(&state, &viewer, id, true).await
}

/// Unstars the issue (ISS-030).
#[utoipa::path(
    delete,
    path = "/issues/{id}/star",
    operation_id = "unstar_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = NO_CONTENT, description = "Unstarred"),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn remove_star(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    star(&state, &viewer, id, false).await
}

async fn vote(
    state: &AppState,
    viewer: &Viewer,
    id: IssueId,
    on: bool,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    issues::set_vote(&state.db, &viewer.access, id, on).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Votes for the issue (ISS-032); not for one's own.
#[utoipa::path(
    put,
    path = "/issues/{id}/vote",
    operation_id = "vote_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = NO_CONTENT, description = "Voted"),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
        (status = CONFLICT, description = "The caller reported it, or it is a draft", body = ErrorBody),
    ),
)]
pub async fn add_vote(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    vote(&state, &viewer, id, true).await
}

/// Withdraws the caller's vote (ISS-032).
#[utoipa::path(
    delete,
    path = "/issues/{id}/vote",
    operation_id = "unvote_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = NO_CONTENT, description = "Withdrawn"),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
        (status = CONFLICT, description = "The caller reported it, or it is a draft", body = ErrorBody),
    ),
)]
pub async fn remove_vote(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    vote(&state, &viewer, id, false).await
}

async fn tag_issue(
    state: &AppState,
    viewer: &Viewer,
    id: IssueId,
    tag: TagId,
    on: bool,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let done = tags::set_on_issue(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        tag,
        on,
    )
    .await?;
    Ok(json(StatusCode::OK, one(state, &viewer.access, &done)))
}

/// Puts one of the caller's tags on the issue (ISS-035).
#[utoipa::path(
    put,
    path = "/issues/{id}/tags/{tag_id}",
    operation_id = "tag_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("tag_id" = i64, Path, description = "Tag ID"),
    ),
    responses(
        (status = OK, description = "The issue now", body = Issue),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or tag", body = ErrorBody),
        (status = CONFLICT, description = "Too many tags, or the project is archived", body = ErrorBody),
    ),
)]
pub async fn add_tag(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, tag)): Path<(IssueId, TagId)>,
) -> Result<Response, ApiError> {
    tag_issue(&state, &viewer, id, tag, true).await
}

/// Takes one of the caller's tags off the issue (ISS-035).
#[utoipa::path(
    delete,
    path = "/issues/{id}/tags/{tag_id}",
    operation_id = "untag_issue",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("tag_id" = i64, Path, description = "Tag ID"),
    ),
    responses(
        (status = OK, description = "The issue now", body = Issue),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or tag", body = ErrorBody),
    ),
)]
pub async fn remove_tag(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, tag)): Path<(IssueId, TagId)>,
) -> Result<Response, ApiError> {
    tag_issue(&state, &viewer, id, tag, false).await
}

fn history_item(
    names: &Names,
    viewer: &Access,
    page: &ServiceActivity,
    entry: &trackflow_store::issue_history::Entry,
    after_comment: Option<i64>,
) -> Option<HistoryItem> {
    let look = Lookups::History(page);
    let kind = ChangeKind::parse(&entry.kind)?;
    let field = entry
        .field_id
        .and_then(|id| page.fields.iter().find(|f| f.id == id));
    let field_kind = field.and_then(|f| FieldKind::parse(&f.kind));
    let side = |json: Option<&Json>| -> ChangeValues {
        let Some(json) = json else {
            return ChangeValues::default();
        };
        match kind {
            ChangeKind::Field => change_values(names, viewer, field_kind, json, &look),
            ChangeKind::Summary | ChangeKind::Description => ChangeValues {
                texts: json.as_str().map(str::to_owned).into_iter().collect(),
                ..ChangeValues::default()
            },
            ChangeKind::Tags => ChangeValues {
                tags: json
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Json::as_i64)
                            .filter_map(|id| page.tags.iter().find(|t| t.id == id))
                            .map(tag_ref)
                            .collect()
                    })
                    .unwrap_or_default(),
                ..ChangeValues::default()
            },
            ChangeKind::Visibility => {
                let ids = |key: &str| -> Vec<i64> {
                    json.get(key)
                        .and_then(Json::as_array)
                        .map(|a| a.iter().filter_map(Json::as_i64).collect())
                        .unwrap_or_default()
                };
                ChangeValues {
                    users: ids("users")
                        .into_iter()
                        .map(|id| match look.user(id) {
                            Some(u) => user_ref(names, viewer, u),
                            None => placeholder(id),
                        })
                        .collect(),
                    groups: ids("groups")
                        .into_iter()
                        .filter_map(|id| look.group(id))
                        .map(group_ref)
                        .collect(),
                    ..ChangeValues::default()
                }
            }
            ChangeKind::Project => ChangeValues {
                place: Some(IssuePlace {
                    project: json
                        .get("project")
                        .and_then(Json::as_i64)
                        .and_then(|id| page.projects.iter().find(|p| p.id == id))
                        .map(project_ref),
                    readable_id: json
                        .get("id")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                }),
                ..ChangeValues::default()
            },
            ChangeKind::Links => ChangeValues {
                links: json
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(LoggedLink::read)
                            .filter_map(|l| {
                                let t = page.link_types.iter().find(|t| t.id == l.link_type)?;
                                let other = page.linked.iter().find(|o| o.id == l.issue)?;
                                Some(ApiLoggedLink {
                                    link_type: link_type_ref(t),
                                    side: side(l.side),
                                    name: side_name(t, l.side).to_owned(),
                                    issue: issue_brief(other, &page.projects)?,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                ..ChangeValues::default()
            },
            ChangeKind::Attachments => ChangeValues {
                files: json
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(read_ref)
                            .map(|(id, name)| LoggedFile {
                                id,
                                name: name.unwrap_or_default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                ..ChangeValues::default()
            },
            ChangeKind::Boards => ChangeValues {
                boards: logged_board(json).into_iter().collect(),
                ..ChangeValues::default()
            },
            _ => ChangeValues::default(),
        }
    };
    Some(HistoryItem {
        id: entry.id,
        at: entry.at,
        author: look.user(entry.author_id).map_or_else(
            || placeholder(entry.author_id),
            |u| user_ref(names, viewer, u),
        ),
        origin: match Origin::parse(&entry.origin) {
            Some(Origin::Api) => HistoryOrigin::Api,
            Some(Origin::System) => HistoryOrigin::System,
            Some(Origin::Rule) => HistoryOrigin::Rule,
            _ => HistoryOrigin::Web,
        },
        silent: entry.silent,
        kind: match kind {
            ChangeKind::Created => HistoryKind::Created,
            ChangeKind::Summary => HistoryKind::Summary,
            ChangeKind::Description => HistoryKind::Description,
            ChangeKind::Field => HistoryKind::Field,
            ChangeKind::Project => HistoryKind::Project,
            ChangeKind::Tags => HistoryKind::Tags,
            ChangeKind::Visibility => HistoryKind::Visibility,
            ChangeKind::Resolved => HistoryKind::Resolved,
            ChangeKind::Reopened => HistoryKind::Reopened,
            ChangeKind::Deleted => HistoryKind::Deleted,
            ChangeKind::Attachments => HistoryKind::Attachments,
            ChangeKind::Links => HistoryKind::Links,
            ChangeKind::Boards => HistoryKind::Boards,
            // Comment records are stream items of their own or none (HIST-006).
            ChangeKind::Comment
            | ChangeKind::CommentEdited
            | ChangeKind::CommentDeleted
            | ChangeKind::CommentRestored => return None,
        },
        field: field.map(field_ref).map(|f: FieldRef| f),
        removed: side(entry.removed.as_ref()),
        added: side(entry.added.as_ref()),
        after_comment,
        // The original a clone's creation names (AUT-066): among the linked issues when the
        // viewer may read it, and the brief says the rest.
        clone_of: (kind == ChangeKind::Created)
            .then(|| entry.added.as_ref().and_then(clone_source))
            .flatten()
            .map(|id| {
                let found = page.linked.iter().find(|l| l.id == id);
                CloneOfRef {
                    id,
                    readable: found.is_some(),
                    issue: found.and_then(|l| issue_brief(l, &page.projects)),
                }
            }),
    })
}

/// The board place of a `boards` record (HIST-002).
fn logged_board(json: &Json) -> Option<LoggedBoard> {
    let named = |item: &Json| {
        read_ref(item).map(|(id, name)| LoggedFile {
            id,
            name: name.unwrap_or_default(),
        })
    };
    Some(LoggedBoard {
        board: named(json.get("board")?)?,
        sprint: json.get("sprint").and_then(named),
    })
}

/// A page of the issue's activity stream: comments and change records (HIST-001…HIST-011,
/// CNT-005, CNT-007), without what the caller may not see.
#[utoipa::path(
    get,
    path = "/issues/{id}/activity",
    operation_id = "issue_activity",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID"), ActivityQuery),
    responses(
        (status = OK, description = "A page of the stream", body = ActivityPage),
        (status = BAD_REQUEST, description = "Invalid limit or filters", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue, or no such comment to go to", body = ErrorBody),
    ),
)]
pub async fn activity(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
    QueryParams(query): QueryParams<ActivityQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let page = activity::page(&state.db, &viewer.access, id, activity_query(&query)?).await?;
    Ok(json(
        StatusCode::OK,
        activity_answer(&state, &viewer.access, &page),
    ))
}

/// What the issue page shows on opening, in one request: the issue as `GET /issues/{id}`, the
/// first page of its activity as `GET /issues/{id}/activity` with the same parameters, its links
/// as `GET /issues/{id}/links` and its files as `GET /issues/{id}/attachments`. The ID may be a
/// readable one, current or former, as for `GET /issues/{id}`.
#[utoipa::path(
    get,
    path = "/issues/{id}/page",
    operation_id = "issue_page",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = String, Path, description = "Internal ID or readable ID such as `WEB-12`"),
        ActivityQuery,
    ),
    responses(
        (status = OK, description = "The page", body = IssuePage),
        (status = BAD_REQUEST, description = "Invalid limit or filters", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue, or no such comment to go to", body = ErrorBody),
    ),
)]
pub async fn page(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<String>,
    QueryParams(query): QueryParams<ActivityQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let query = activity_query(&query)?;
    let id = match id.parse::<IssueId>() {
        Ok(id) => id,
        Err(_) => issues::resolve(&state.db, &viewer.access, &id).await?.0,
    };
    let found = issues::page::page(&state.db, &viewer.access, id, query).await?;
    Ok(json(
        StatusCode::OK,
        IssuePage {
            issue: one(&state, &viewer.access, &found.issue),
            activity: activity_answer(&state, &viewer.access, &found.activity),
            links: crate::issue_links::panel(&found.links),
            attachments: crate::files::page(&state, &viewer.access, &found.attachments),
            boards: found
                .boards
                .iter()
                .map(crate::boards::issue_board)
                .collect(),
        },
    ))
}

/// The service's query from the HTTP one, or why it is invalid.
fn activity_query(query: &ActivityQuery) -> Result<activity::Query, ApiError> {
    let limit = match query.limit {
        None => HISTORY_DEFAULT_LIMIT,
        Some(limit) if (1..=100).contains(&limit) => i64::from(limit),
        Some(_) => {
            return Err(ApiError::invalid(vec![
                trackflow_api::error::FieldError::new(
                    "limit",
                    "out_of_range",
                    "limit must be between 1 and 100",
                ),
            ]));
        }
    };
    let comments = query.comments.unwrap_or(true);
    let changes = query.changes.unwrap_or(true);
    if !comments && !changes {
        return Err(ApiError::invalid(vec![
            trackflow_api::error::FieldError::new(
                "changes",
                "required",
                "show comments, changes or both",
            ),
        ]));
    }
    let direction = match query.direction.unwrap_or_default() {
        ActivityDirection::Older => Direction::Older,
        ActivityDirection::Newer => Direction::Newer,
    };
    Ok(activity::Query {
        comments,
        changes,
        from: query.from,
        direction,
        limit,
        around_comment: query.around_comment,
    })
}

/// A page of the stream as it goes out.
fn activity_answer(state: &AppState, viewer: &Access, page: &ServiceActivity) -> ActivityPage {
    let names = state.names();
    let signer = state.signer();
    let items = page
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Change {
                entry,
                after_comment,
            } => history_item(&names, viewer, page, entry, *after_comment)
                .map(|h| ActivityItem::Change(Box::new(h))),
            Item::Comment { entry, comment } => {
                Some(ActivityItem::Comment(Box::new(CommentItem {
                    id: entry.id,
                    at: entry.at,
                    comment: comment_from(
                        &names,
                        &signer,
                        viewer,
                        comment,
                        &|id| page.users.iter().find(|u| u.id == id),
                        &|id| page.groups.iter().find(|g| g.id == id),
                        &page.refs,
                        page.files
                            .iter()
                            .filter(|f| f.file.comment_id == Some(comment.comment.id)),
                    ),
                })))
            }
        })
        .collect();
    ActivityPage {
        items,
        older: page.older,
        newer: page.newer,
    }
}

/// Whom the mention list offers while typing `@` in an issue's text (CNT-054): people who can see
/// the issue. Empty for callers who may not see the directory.
#[utoipa::path(
    get,
    path = "/issues/{id}/mention-candidates",
    operation_id = "mention_candidates",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID"), MentionQuery),
    responses(
        (status = OK, description = "Candidates", body = MentionCandidates),
        (status = BAD_REQUEST, description = "Invalid limit", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn mention_candidates(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
    QueryParams(query): QueryParams<MentionQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let limit = match query.limit {
        None => 10,
        Some(limit) if (1..=20).contains(&limit) => i64::from(limit),
        Some(_) => {
            return Err(ApiError::invalid(vec![
                trackflow_api::error::FieldError::new(
                    "limit",
                    "out_of_range",
                    "limit must be between 1 and 20",
                ),
            ]));
        }
    };
    let found = trackflow_service::people::mention_candidates(
        &state.db,
        &viewer.access,
        id,
        query.q.as_deref().unwrap_or_default(),
        limit,
    )
    .await?;
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        MentionCandidates {
            users: found
                .iter()
                .map(|u| user_ref(&names, &viewer.access, u))
                .collect(),
        },
    ))
}

/// Whom the audience picker offers in a project (ISS-043).
#[utoipa::path(
    get,
    path = "/projects/{id}/audience-candidates",
    operation_id = "audience_candidates",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID"), CandidateQuery),
    responses(
        (status = OK, description = "Candidates", body = AudienceCandidates),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn candidates(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    QueryParams(query): QueryParams<CandidateQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = issues::audience::candidates(
        &state.db,
        &viewer.access,
        id,
        query.q.as_deref().unwrap_or_default(),
    )
    .await?;
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        AudienceCandidates {
            groups: found.groups.iter().map(group_ref).collect(),
            users: found
                .users
                .iter()
                .map(|u| user_ref(&names, &viewer.access, u))
                .collect(),
        },
    ))
}

/// Tags by name (ISS-035): the caller's own, others' shared with them (ISS-037), or both.
#[utoipa::path(
    get,
    path = "/tags",
    operation_id = "list_tags",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(TagQuery, PageQuery),
    responses(
        (status = OK, description = "A page of tags", body = Page<Tag>),
        (status = BAD_REQUEST, description = "Invalid limit or cursor", body = ErrorBody),
    ),
)]
pub async fn list_tags(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<TagQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let page = query
        .resolve::<(String, TagId)>(TAGS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let rows = tags::list(
        &state.db,
        &viewer.access,
        tag_listing(filter.scope),
        filter.q.as_deref().unwrap_or_default(),
        page.after.clone(),
        page.fetch_limit(),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |t| (t.name.to_lowercase(), t.id));
    Ok(json(
        StatusCode::OK,
        Page {
            items: tags_shown(&state, &viewer.access, &page.items).await?,
            next_cursor: page.next_cursor,
        },
    ))
}

/// Creates a tag for the caller (ISS-034).
#[utoipa::path(
    post,
    path = "/tags",
    operation_id = "create_tag",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = NewTag,
    responses(
        (status = CREATED, description = "Created", body = Tag),
        (status = BAD_REQUEST, description = "Invalid or taken name", body = ErrorBody),
        (status = CONFLICT, description = "Too many tags", body = ErrorBody),
    ),
)]
pub async fn create_tag(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewTag>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let created = tags::create(&state.db, &viewer.access, &request.name, request.color).await?;
    Ok(json(
        StatusCode::CREATED,
        tag_shown(&state, &viewer.access, created).await?,
    ))
}

/// Renames or recolors a tag (ISS-036), shares it or gives it to another owner (ISS-037,
/// ACC-066). Sharing without the share-views capability or the rights answers `forbidden`.
#[utoipa::path(
    patch,
    path = "/tags/{id}",
    operation_id = "update_tag",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Tag ID")),
    request_body = TagPatch,
    responses(
        (status = OK, description = "The tag now", body = Tag),
        (status = BAD_REQUEST, description = "Invalid or taken name", body = ErrorBody),
        (status = FORBIDDEN, description = "May not edit or share it", body = ErrorBody),
        (status = NOT_FOUND, description = "No such tag", body = ErrorBody),
        (status = CONFLICT, description = "Changed by someone else", body = ErrorBody),
    ),
)]
pub async fn update_tag(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<TagId>,
    JsonBody(patch): JsonBody<TagPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let updated = tags::update(
        &state.db,
        &viewer.access,
        id,
        tags::TagChange {
            name: patch.name,
            color: patch.color,
            sharing: patch.sharing.map(|s| tags::Sharing {
                view: shared_with(s.view),
                use_: shared_with(s.use_),
                edit: shared_with(s.edit),
            }),
            owner: patch.owner,
            version: patch.version,
        },
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        tag_shown(&state, &viewer.access, updated).await?,
    ))
}

/// One tag the caller owns or that is shared with them (QRY-053: the issue list's context).
#[utoipa::path(
    get,
    path = "/tags/{id}",
    operation_id = "get_tag",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Tag ID")),
    responses(
        (status = OK, description = "The tag", body = Tag),
        (status = NOT_FOUND, description = "No such tag", body = ErrorBody),
    ),
)]
pub async fn get_tag(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<TagId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = tags::get(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        tag_shown(&state, &viewer.access, found).await?,
    ))
}

/// How many issues carry a tag: shown before deleting it (ISS-036).
#[utoipa::path(
    get,
    path = "/tags/{id}/usage",
    operation_id = "get_tag_usage",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Tag ID")),
    responses(
        (status = OK, description = "Issues carrying it", body = IssueImpact),
        (status = NOT_FOUND, description = "No such tag", body = ErrorBody),
    ),
)]
pub async fn tag_usage(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<TagId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let issues = tags::usage(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, IssueImpact { issues }))
}

/// Deletes a tag; it leaves every issue (ISS-036).
#[utoipa::path(
    delete,
    path = "/tags/{id}",
    operation_id = "delete_tag",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Tag ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = NOT_FOUND, description = "No such tag", body = ErrorBody),
    ),
)]
pub async fn delete_tag(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<TagId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    tags::delete(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
