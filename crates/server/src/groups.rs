//! `/groups`: the list, creating and editing groups, members, deleting with a replacement
//! (ACC-021…ACC-032).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use trackflow_api::access::{
    ChangedMembers, Group, GroupDeletion, GroupDeletionImpact, GroupKind, GroupPatch, GroupQuery,
    GroupRole, GroupSelection, MembersChange, NewGroup, TextQuery,
};
use trackflow_api::accounts::UserRef;
use trackflow_api::error::ErrorBody;
use trackflow_api::page::{Page, PageQuery};
use trackflow_domain::auth::Scope;
use trackflow_domain::{GroupId, UserId};
use trackflow_service::groups::{self, GroupChange, GroupRow};

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::present::{builtin_group, role_ref, user_ref};

const GROUPS_MAX_LIMIT: u32 = 100;
const MEMBERS_MAX_LIMIT: u32 = 100;

fn group(row: &GroupRow) -> Group {
    let group = &row.listed.group;
    Group {
        id: group.id,
        name: group.name.clone(),
        description: group.description.clone(),
        builtin: builtin_group(group.builtin.as_deref()),
        auto_join: group.auto_join,
        team_of: group.team_of,
        members: row.listed.members,
        roles: row
            .assignments
            .iter()
            .map(|(assignment, role)| GroupRole {
                assignment_id: assignment.id,
                role: role_ref(role),
                project_id: assignment.project_id,
            })
            .collect(),
        version: group.version,
        created_at: group.created_at,
    }
}

/// Groups, built-in ones first, then by name (ACC-022): groups of people, project teams or both.
/// Needs `system.read` or `project.update` in a project.
#[utoipa::path(
    get,
    path = "/groups",
    operation_id = "list_groups",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    params(GroupQuery, PageQuery),
    responses(
        (status = OK, description = "A page of groups", body = Page<Group>),
        (status = BAD_REQUEST, description = "Invalid limit or cursor", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<GroupQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let page = query
        .resolve::<(i32, String, GroupId)>(GROUPS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let kinds = match filter.kind.unwrap_or_default() {
        GroupKind::Groups => trackflow_store::groups::Kinds {
            groups: true,
            teams: false,
        },
        GroupKind::Teams => trackflow_store::groups::Kinds {
            groups: false,
            teams: true,
        },
        GroupKind::All => trackflow_store::groups::Kinds {
            groups: true,
            teams: true,
        },
    };
    let rows = groups::list(
        &state.db,
        &viewer.access,
        filter.q.as_deref().unwrap_or_default(),
        kinds,
        page.after.clone(),
        page.fetch_limit(),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |row| {
        let g = &row.listed.group;
        (i32::from(g.builtin.is_none()), g.name.clone(), g.id)
    });
    Ok(json(
        StatusCode::OK,
        Page {
            items: page.items.iter().map(group).collect::<Vec<_>>(),
            next_cursor: page.next_cursor,
        },
    ))
}

/// Creates an empty group (ACC-023).
#[utoipa::path(
    post,
    path = "/groups",
    operation_id = "create_group",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    request_body = NewGroup,
    responses(
        (status = CREATED, description = "Created", body = Group),
        (status = BAD_REQUEST, description = "Invalid or taken name", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewGroup>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let created = groups::create(
        &state.db,
        &viewer.access,
        groups::NewGroup {
            name: request.name,
            description: request.description,
            auto_join: request.auto_join,
        },
    )
    .await?;
    let row = groups::get(&state.db, &viewer.access, created.id).await?;
    Ok(json(StatusCode::CREATED, group(&row)))
}

/// One group; the ID of a deleted group answers with its replacement (ACC-013).
#[utoipa::path(
    get,
    path = "/groups/{id}",
    operation_id = "get_group",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Group ID")),
    responses(
        (status = OK, description = "The group", body = Group),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such group", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let row = groups::get(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, group(&row)))
}

/// Changes name, description and auto-join (ACC-024).
#[utoipa::path(
    patch,
    path = "/groups/{id}",
    operation_id = "update_group",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Group ID")),
    request_body = GroupPatch,
    responses(
        (status = OK, description = "Changed", body = Group),
        (status = BAD_REQUEST, description = "Invalid or taken name", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such group", body = ErrorBody),
        (status = CONFLICT, description = "Changed meanwhile, or a built-in group's name", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<GroupPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    groups::update(
        &state.db,
        &viewer.access,
        id,
        GroupChange {
            name: request.name,
            description: request.description,
            auto_join: request.auto_join,
            version: request.version,
        },
    )
    .await?;
    let row = groups::get(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, group(&row)))
}

/// Members by name (ACC-026); built-in groups list every account they include.
#[utoipa::path(
    get,
    path = "/groups/{id}/members",
    operation_id = "list_group_members",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Group ID"), TextQuery, PageQuery),
    responses(
        (status = OK, description = "A page of members", body = Page<UserRef>),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such group", body = ErrorBody),
    ),
)]
pub async fn members(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    QueryParams(filter): QueryParams<TextQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let page = query
        .resolve::<(String, UserId)>(MEMBERS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let rows = groups::members(
        &state.db,
        &viewer.access,
        id,
        filter.q.as_deref().unwrap_or_default(),
        page.after.clone(),
        page.fetch_limit(),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |user| (user.full_name.clone(), user.id));
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        Page {
            items: page
                .items
                .iter()
                .map(|user| user_ref(&names, &viewer.access, user))
                .collect::<Vec<_>>(),
            next_cursor: page.next_cursor,
        },
    ))
}

/// Adds accounts (ACC-026); needs every permission the group gives (ACC-050).
#[utoipa::path(
    post,
    path = "/groups/{id}/members",
    operation_id = "add_group_members",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Group ID")),
    request_body = MembersChange,
    responses(
        (status = OK, description = "Accounts that were added", body = ChangedMembers),
        (status = FORBIDDEN, description = "Missing permissions, named in the answer", body = ErrorBody),
        (status = NOT_FOUND, description = "No such group or account", body = ErrorBody),
        (status = CONFLICT, description = "A built-in group", body = ErrorBody),
    ),
)]
pub async fn add_members(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<MembersChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let users = groups::add_members(&state.db, &viewer.access, id, &request.users).await?;
    state.access.clear();
    Ok(json(StatusCode::OK, ChangedMembers { users }))
}

/// Removes accounts (ACC-026).
#[utoipa::path(
    post,
    path = "/groups/{id}/members/remove",
    operation_id = "remove_group_members",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Group ID")),
    request_body = MembersChange,
    responses(
        (status = OK, description = "Accounts that were removed", body = ChangedMembers),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such group", body = ErrorBody),
        (status = CONFLICT, description = "A built-in group, or the last administrator", body = ErrorBody),
    ),
)]
pub async fn remove_members(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<MembersChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let users = groups::remove_members(&state.db, &viewer.access, id, &request.users).await?;
    state.access.clear();
    Ok(json(StatusCode::OK, ChangedMembers { users }))
}

/// What deleting groups would take away (ACC-032).
#[utoipa::path(
    post,
    path = "/groups/deletion-impact",
    operation_id = "group_deletion_impact",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    request_body = GroupSelection,
    responses(
        (status = OK, description = "Counts", body = GroupDeletionImpact),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
    ),
)]
pub async fn deletion_impact(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<GroupSelection>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let impact = groups::deletion_impact(&state.db, &viewer.access, &request.groups).await?;
    Ok(json(
        StatusCode::OK,
        GroupDeletionImpact {
            groups: impact.groups,
            memberships: impact.memberships,
            assignments: impact.assignments,
        },
    ))
}

/// Deletes groups (ACC-032): roles revoked, members leave, IDs resolve to the replacement.
#[utoipa::path(
    post,
    path = "/groups/delete",
    operation_id = "delete_groups",
    tag = "groups",
    security(("session" = []), ("token" = [])),
    request_body = GroupDeletion,
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = BAD_REQUEST, description = "The replacement is among the deleted", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such group", body = ErrorBody),
        (status = CONFLICT, description = "A built-in group, or the last administrator", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<GroupDeletion>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    groups::delete(
        &state.db,
        &viewer.access,
        &request.groups,
        request.replacement,
    )
    .await?;
    state.access.clear();
    Ok(StatusCode::NO_CONTENT.into_response())
}
