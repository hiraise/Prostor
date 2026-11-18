//! `/link-types` and `/issues/{id}/links`: link types and their administration, the links of an
//! issue (LNK). Rules and flows live in `taskmaster_service::issue_links`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use taskmaster_api::error::ErrorBody;
use taskmaster_api::links::{
    AddLinks, BuiltinLinkType, IssueBrief, IssueLinks, LinkDirection, LinkGroup, LinkSide,
    LinkType, LinkTypeImpact, LinkTypeList, LinkTypePatch, LinkTypeRef, LinkedIssue, LinksAdded,
    LinksRemoved, NewLinkType, RemoveLinkQuery,
};
use taskmaster_api::page::{PageQuery, encode_cursor};
use taskmaster_domain::auth::Scope;
use taskmaster_domain::issues;
use taskmaster_domain::links::{BuiltinType, Direction, Side};
use taskmaster_domain::{IssueId, LinkId, LinkTypeId};
use taskmaster_service::issue_links::{
    self as service, Group, GroupCursor, NewLinks, Panel, TypeInput, TypeRow, TypeUpdate,
};
use taskmaster_store::links::{LinkType as StoredType, Linked};
use taskmaster_store::projects::Project;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::issues::origin;
use crate::present::{project_ref, rule_messages};

const GROUP_MAX_LIMIT: u32 = 100;

pub fn direction(d: Direction) -> LinkDirection {
    match d {
        Direction::Undirected => LinkDirection::Undirected,
        Direction::Directed => LinkDirection::Directed,
        Direction::Hierarchy => LinkDirection::Hierarchy,
    }
}

fn domain_direction(d: LinkDirection) -> Direction {
    match d {
        LinkDirection::Undirected => Direction::Undirected,
        LinkDirection::Directed => Direction::Directed,
        LinkDirection::Hierarchy => Direction::Hierarchy,
    }
}

pub fn side(s: Side) -> LinkSide {
    match s {
        Side::Source => LinkSide::Source,
        Side::Target => LinkSide::Target,
    }
}

pub fn domain_side(s: LinkSide) -> Side {
    match s {
        LinkSide::Source => Side::Source,
        LinkSide::Target => Side::Target,
    }
}

fn builtin(key: Option<&str>) -> Option<BuiltinLinkType> {
    match BuiltinType::parse(key?)? {
        BuiltinType::Related => Some(BuiltinLinkType::Related),
        BuiltinType::Dependency => Some(BuiltinLinkType::Dependency),
        BuiltinType::Duplicate => Some(BuiltinLinkType::Duplicate),
        BuiltinType::Subtask => Some(BuiltinLinkType::Subtask),
    }
}

pub fn link_type_ref(t: &StoredType) -> LinkTypeRef {
    LinkTypeRef {
        id: t.id,
        name: t.name.clone(),
        source_name: t.source_name.clone(),
        target_name: t.target_name.clone(),
        direction: direction(service::direction_of(t)),
        builtin: builtin(t.builtin.as_deref()),
    }
}

fn link_type(row: &TypeRow) -> LinkType {
    let t = &row.link_type;
    LinkType {
        id: t.id,
        name: t.name.clone(),
        source_name: t.source_name.clone(),
        target_name: t.target_name.clone(),
        direction: direction(service::direction_of(t)),
        builtin: builtin(t.builtin.as_deref()),
        locked: t.locked,
        deleted_at: t.deleted_at,
        usage: row.usage,
        version: t.version,
    }
}

/// A linked issue as answers show it; `None` when its project is not among those given.
pub fn issue_brief(l: &Linked, projects: &[Project]) -> Option<IssueBrief> {
    let project = projects.iter().find(|p| p.id == l.project_id)?;
    Some(IssueBrief {
        id: l.id,
        readable_id: issues::readable_id(&project.key, l.number?),
        summary: l.summary.clone(),
        project: project_ref(project),
        resolved: l.resolved_at.is_some(),
        restricted: !l.audience_users.is_empty() || !l.audience_groups.is_empty(),
    })
}

fn group(g: &Group, projects: &[Project]) -> LinkGroup {
    LinkGroup {
        link_type: link_type_ref(&g.link_type),
        side: side(g.side),
        name: service::side_name(&g.link_type, g.side).to_owned(),
        total: g.total,
        unresolved: g.unresolved,
        items: g
            .rows
            .iter()
            .filter_map(|r| {
                Some(LinkedIssue {
                    link: r.link,
                    linked_at: r.created_at,
                    issue: issue_brief(&r.issue, projects)?,
                })
            })
            .collect(),
        next_cursor: g.next.map(|c| encode_cursor(&(c.resolved, c.link))),
    }
}

pub(crate) fn panel(p: &Panel) -> IssueLinks {
    IssueLinks {
        groups: p.groups.iter().map(|g| group(g, &p.projects)).collect(),
        can_link: p.can_link,
    }
}

/// The link types (LNK-001, LNK-018): those in use for everyone signed in; deleted ones and usage
/// counts for holders of `system.read`.
#[utoipa::path(
    get,
    path = "/link-types",
    operation_id = "list_link_types",
    tag = "links",
    security(("session" = []), ("token" = [])),
    responses((status = OK, description = "The link types, oldest first", body = LinkTypeList)),
)]
pub async fn list_types(
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let rows = service::list_types(&state.db, &viewer.access).await?;
    Ok(json(
        StatusCode::OK,
        LinkTypeList {
            items: rows.iter().map(link_type).collect(),
        },
    ))
}

/// Creates a link type (LNK-019). Needs `system.write`.
#[utoipa::path(
    post,
    path = "/link-types",
    operation_id = "create_link_type",
    tag = "links",
    security(("session" = []), ("token" = [])),
    request_body = NewLinkType,
    responses(
        (status = CREATED, description = "Created", body = LinkType),
        (status = BAD_REQUEST, description = "Invalid or taken names", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
    ),
)]
pub async fn create_type(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewLinkType>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = service::create_type(
        &state.db,
        &viewer.access,
        TypeInput {
            name: request.name,
            source_name: request.source_name,
            target_name: request.target_name,
            direction: domain_direction(request.direction),
        },
    )
    .await?;
    Ok(json(StatusCode::CREATED, link_type(&row)))
}

/// Changes a link type (LNK-020): renames apply to every link at once. Built-in locked types do
/// not change. Needs `system.write`.
#[utoipa::path(
    patch,
    path = "/link-types/{id}",
    operation_id = "update_link_type",
    tag = "links",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Link type ID")),
    request_body = LinkTypePatch,
    responses(
        (status = OK, description = "Changed", body = LinkType),
        (status = BAD_REQUEST, description = "Invalid or taken names", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such link type", body = ErrorBody),
        (status = CONFLICT, description = "Changed meanwhile, deleted, locked (`link_type_locked`) or its links break the hierarchy rules (`hierarchy_conflict`)", body = ErrorBody),
    ),
)]
pub async fn update_type(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<LinkTypeId>,
    JsonBody(request): JsonBody<LinkTypePatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = service::update_type(
        &state.db,
        &viewer.access,
        id,
        TypeUpdate {
            name: request.name,
            source_name: request.source_name,
            target_name: request.target_name,
            direction: request.direction.map(domain_direction),
            version: request.version,
        },
    )
    .await?;
    Ok(json(StatusCode::OK, link_type(&row)))
}

/// How many links deleting a link type hides (LNK-021). Needs `system.write`.
#[utoipa::path(
    get,
    path = "/link-types/{id}/impact",
    operation_id = "link_type_impact",
    tag = "links",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Link type ID")),
    responses(
        (status = OK, description = "What deleting it hides", body = LinkTypeImpact),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such link type", body = ErrorBody),
    ),
)]
pub async fn type_impact(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<LinkTypeId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let links = service::deletion_impact(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, LinkTypeImpact { links }))
}

/// Puts a link type in the trash (LNK-021): its links are hidden until it is restored. Needs
/// `system.write`.
#[utoipa::path(
    delete,
    path = "/link-types/{id}",
    operation_id = "delete_link_type",
    tag = "links",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Link type ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such link type", body = ErrorBody),
        (status = CONFLICT, description = "Built in and locked (`link_type_locked`), or deleted already", body = ErrorBody),
    ),
)]
pub async fn delete_type(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<LinkTypeId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    service::delete_type(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Takes a link type out of the trash with its links. Needs `system.write`.
#[utoipa::path(
    post,
    path = "/link-types/{id}/restore",
    operation_id = "restore_link_type",
    tag = "links",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Link type ID")),
    responses(
        (status = OK, description = "Restored", body = LinkType),
        (status = BAD_REQUEST, description = "Another type took one of its names meanwhile", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such link type", body = ErrorBody),
        (status = CONFLICT, description = "Not deleted", body = ErrorBody),
    ),
)]
pub async fn restore_type(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<LinkTypeId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = service::restore_type(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, link_type(&row)))
}

/// An issue's links the caller may see (LNK-004, LNK-010): grouped by link type and side,
/// unresolved first, the first ten of each group.
#[utoipa::path(
    get,
    path = "/issues/{id}/links",
    operation_id = "issue_links",
    tag = "links",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = OK, description = "The links", body = IssueLinks),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = service::panel(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, panel(&found)))
}

/// A page of one group of an issue's links (LNK-010): one link type seen from one side.
#[utoipa::path(
    get,
    path = "/issues/{id}/links/{link_type}/{side}",
    operation_id = "issue_link_group",
    tag = "links",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("link_type" = i64, Path, description = "Link type ID"),
        ("side" = LinkSide, Path, description = "The issue's side; `source` for undirected types"),
        PageQuery,
    ),
    responses(
        (status = OK, description = "A page of the group", body = LinkGroup),
        (status = BAD_REQUEST, description = "Invalid limit or cursor", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or link type", body = ErrorBody),
    ),
)]
pub async fn group_page(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, link_type, link_side)): Path<(IssueId, LinkTypeId, LinkSide)>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let page = query
        .resolve::<(bool, LinkId)>(GROUP_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let found = service::group_page(
        &state.db,
        &viewer.access,
        id,
        link_type,
        domain_side(link_side),
        page.after
            .map(|(resolved, link)| GroupCursor { resolved, link }),
        i64::from(page.limit),
    )
    .await?;
    Ok(json(StatusCode::OK, group(&found.0, &found.1)))
}

/// Links issues to this one (LNK-003, LNK-005, LNK-013): `issue.link` here (or being its
/// reporter), reading the others, both projects active. Pairs linked already are reported in
/// `existing`.
#[utoipa::path(
    post,
    path = "/issues/{id}/links",
    operation_id = "add_issue_links",
    tag = "links",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    request_body = AddLinks,
    responses(
        (status = OK, description = "Linked", body = LinksAdded),
        (status = BAD_REQUEST, description = "No issues, too many, or the issue itself", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or link type", body = ErrorBody),
        (status = CONFLICT, description = "An archived project, a second parent (`parent_taken`), a loop (`link_cycle`), or too many links", body = ErrorBody),
    ),
)]
pub async fn add(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
    JsonBody(request): JsonBody<AddLinks>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let added = service::add(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        NewLinks {
            link_type: request.link_type,
            side: domain_side(request.side),
            issues: request.issues,
        },
        request.silent,
    )
    .await?;
    let found = service::panel(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        LinksAdded {
            added: added.done.added,
            existing: added.done.existing,
            links: panel(&found),
            messages: rule_messages(&added.messages),
        },
    ))
}

/// Removes a link from both of its issues (LNK-008).
#[utoipa::path(
    delete,
    path = "/issues/{id}/links/{link}",
    operation_id = "remove_issue_link",
    tag = "links",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("link" = i64, Path, description = "Link ID"),
        RemoveLinkQuery,
    ),
    responses(
        (status = OK, description = "Removed, with what the rules said", body = LinksRemoved),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or link", body = ErrorBody),
        (status = CONFLICT, description = "An archived project", body = ErrorBody),
    ),
)]
pub async fn remove(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, link)): Path<(IssueId, LinkId)>,
    QueryParams(query): QueryParams<RemoveLinkQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let messages = service::remove(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        link,
        query.silent.unwrap_or(false),
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        LinksRemoved {
            messages: rule_messages(&messages),
        },
    ))
}
