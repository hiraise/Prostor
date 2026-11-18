//! `/permissions`, `/roles`, `/role-assignments`, `/capabilities` and the caller's access map
//! (ACC-034…ACC-064).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use prostor_api::access::{
    AccessMap, Assignment, AssignmentQuery, Assignments, Capabilities, Capability,
    CapabilityChange, CapabilityKey, GroupRef, Holder, NewAssignment, NewRole, PermissionCatalog,
    PermissionKey, ProjectAccess, Role, RolePatch, RoleQuery,
};
use prostor_api::error::ErrorBody;
use prostor_api::page::{Page, PageQuery};
use prostor_domain::RoleId;
use prostor_domain::access::Access;
use prostor_domain::access::capability;
use prostor_domain::access::permission::Permission;
use prostor_domain::auth::Scope;
use prostor_service::people::Names;
use prostor_service::roles::{self, Described};
use prostor_store::access as store_access;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::{Caller, require_scope};
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::present::{self, group_ref, permission_info, permission_keys, role, role_ref, user_ref};

const ROLES_MAX_LIMIT: u32 = 100;
const ASSIGNMENTS_MAX_LIMIT: u32 = 200;

/// The permission catalog (ACC-034): every permission with what it implies. Needs only a sign-in.
#[utoipa::path(
    get,
    path = "/permissions",
    operation_id = "list_permissions",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    responses((status = OK, description = "The catalog", body = PermissionCatalog)),
)]
pub async fn catalog(_caller: Caller) -> Result<Response, ApiError> {
    Ok(json(
        StatusCode::OK,
        PermissionCatalog {
            permissions: Permission::ALL
                .into_iter()
                .filter_map(permission_info)
                .collect(),
        },
    ))
}

/// Roles, predefined ones first (ACC-053).
#[utoipa::path(
    get,
    path = "/roles",
    operation_id = "list_roles",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(RoleQuery, PageQuery),
    responses(
        (status = OK, description = "A page of roles", body = Page<Role>),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<RoleQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let page = query
        .resolve::<RoleId>(ROLES_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let rows = roles::list(
        &state.db,
        &viewer.access,
        filter.q.as_deref().unwrap_or_default(),
        filter.permission.and_then(present::permission),
        page.after,
        usize::try_from(page.fetch_limit()).unwrap_or(usize::MAX),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |row| row.role.id);
    Ok(json(
        StatusCode::OK,
        Page {
            items: page.items.iter().map(role).collect::<Vec<_>>(),
            next_cursor: page.next_cursor,
        },
    ))
}

/// Creates a custom role (ACC-054); needs `system.write` and every permission it gives.
#[utoipa::path(
    post,
    path = "/roles",
    operation_id = "create_role",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    request_body = NewRole,
    responses(
        (status = CREATED, description = "Created", body = Role),
        (status = BAD_REQUEST, description = "Invalid or taken name", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions, named in the answer", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewRole>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = roles::create(
        &state.db,
        &viewer.access,
        roles::NewRole {
            name: request.name,
            description: request.description,
            permissions: request.permissions.iter().map(|key| key.key()).collect(),
        },
    )
    .await?;
    Ok(json(StatusCode::CREATED, role(&row)))
}

/// One role.
#[utoipa::path(
    get,
    path = "/roles/{id}",
    operation_id = "get_role",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Role ID")),
    responses(
        (status = OK, description = "The role", body = Role),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such role", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let row = roles::get(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, role(&row)))
}

/// Renames a custom role or changes its description (ACC-054).
#[utoipa::path(
    patch,
    path = "/roles/{id}",
    operation_id = "update_role",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Role ID")),
    request_body = RolePatch,
    responses(
        (status = OK, description = "Changed", body = Role),
        (status = BAD_REQUEST, description = "Invalid or taken name", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such role", body = ErrorBody),
        (status = CONFLICT, description = "A predefined role, or changed meanwhile", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<RolePatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = roles::update(
        &state.db,
        &viewer.access,
        id,
        request.name.as_deref(),
        request.description.as_deref(),
        request.version,
    )
    .await?;
    Ok(json(StatusCode::OK, role(&row)))
}

/// Deletes a custom role and revokes its assignments (ACC-057).
#[utoipa::path(
    delete,
    path = "/roles/{id}",
    operation_id = "delete_role",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Role ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such role", body = ErrorBody),
        (status = CONFLICT, description = "A predefined role", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    roles::delete(&state.db, &viewer.access, id).await?;
    state.access.clear();
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Switches a permission on in a custom role, with what it implies (ACC-046, ACC-054).
#[utoipa::path(
    put,
    path = "/roles/{id}/permissions/{permission}",
    operation_id = "grant_role_permission",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Role ID"),
        ("permission" = PermissionKey, Path, description = "Permission key"),
    ),
    responses(
        (status = OK, description = "The role now", body = Role),
        (status = FORBIDDEN, description = "Missing permissions, named in the answer", body = ErrorBody),
        (status = NOT_FOUND, description = "No such role", body = ErrorBody),
        (status = CONFLICT, description = "A predefined role", body = ErrorBody),
    ),
)]
pub async fn grant_permission(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, key)): Path<(i64, String)>,
) -> Result<Response, ApiError> {
    toggle(state, viewer, id, &key, true).await
}

/// Switches a permission off in a custom role, with what depends on it (ACC-046, ACC-054).
#[utoipa::path(
    delete,
    path = "/roles/{id}/permissions/{permission}",
    operation_id = "revoke_role_permission",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Role ID"),
        ("permission" = PermissionKey, Path, description = "Permission key"),
    ),
    responses(
        (status = OK, description = "The role now", body = Role),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such role", body = ErrorBody),
        (status = CONFLICT, description = "A predefined role", body = ErrorBody),
    ),
)]
pub async fn revoke_permission(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, key)): Path<(i64, String)>,
) -> Result<Response, ApiError> {
    toggle(state, viewer, id, &key, false).await
}

async fn toggle(
    state: AppState,
    viewer: Viewer,
    id: i64,
    key: &str,
    on: bool,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let permission =
        Permission::parse(key).ok_or_else(|| ApiError::not_found("No such permission"))?;
    let row = roles::set_permission(&state.db, &viewer.access, id, permission, on).await?;
    state.access.clear();
    Ok(json(StatusCode::OK, role(&row)))
}

fn assignment(names: &Names, viewer: &Access, described: &Described) -> Option<Assignment> {
    let holder = match (&described.user, &described.group) {
        (Some(user), _) => Holder::User {
            user: user_ref(names, viewer, user),
        },
        (None, Some(group)) => Holder::Group {
            group: group_ref(group),
        },
        (None, None) => return None,
    };
    Some(Assignment {
        id: described.assignment.id,
        role: role_ref(&described.role),
        holder,
        project_id: described.assignment.project_id,
        created_at: described.assignment.created_at,
    })
}

/// Assignments as the viewer sees their holders.
pub fn assignment_list(names: &Names, viewer: &Access, list: &[Described]) -> Vec<Assignment> {
    list.iter()
        .filter_map(|described| assignment(names, viewer, described))
        .collect()
}

/// Every assignment of a role: what deleting it revokes (ACC-057).
#[utoipa::path(
    get,
    path = "/roles/{id}/assignments",
    operation_id = "list_role_assignments",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Role ID"), PageQuery),
    responses(
        (status = OK, description = "A page of assignments", body = Page<Assignment>),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such role", body = ErrorBody),
    ),
)]
pub async fn role_assignments(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let page = query
        .resolve::<i64>(ASSIGNMENTS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let all = roles::assignments_of(&state.db, &viewer.access, id).await?;
    let start = page
        .after
        .map_or(0, |after| all.iter().take_while(|a| a.id <= after).count());
    let slice: Vec<_> = all
        .into_iter()
        .skip(start)
        .take(usize::try_from(page.fetch_limit()).unwrap_or(usize::MAX))
        .collect();
    let described = roles::describe(&state.db, slice).await?;
    let names = state.names();
    let page = Page::from_rows(described, page.limit, |d| d.assignment.id);
    Ok(json(
        StatusCode::OK,
        Page {
            items: assignment_list(&names, &viewer.access, &page.items),
            next_cursor: page.next_cursor,
        },
    ))
}

/// The direct assignments of an account or a group (ACC-059, ACC-060).
#[utoipa::path(
    get,
    path = "/role-assignments",
    operation_id = "list_assignments",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(AssignmentQuery),
    responses(
        (status = OK, description = "The holder's assignments", body = Assignments),
        (status = BAD_REQUEST, description = "Neither or both of user and group", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
    ),
)]
pub async fn list_assignments(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<AssignmentQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let holder = holder(query.user, query.group)?;
    let list = roles::assignments_of_holder(&state.db, &viewer.access, holder).await?;
    let described = roles::describe(&state.db, list).await?;
    Ok(json(
        StatusCode::OK,
        Assignments {
            items: assignment_list(&state.names(), &viewer.access, &described),
        },
    ))
}

fn holder(user: Option<i64>, group: Option<i64>) -> Result<store_access::Holder, ApiError> {
    match (user, group) {
        (Some(user), None) => Ok(store_access::Holder::User(user)),
        (None, Some(group)) => Ok(store_access::Holder::Group(group)),
        _ => Err(ApiError::invalid(vec![
            prostor_api::error::FieldError::new(
                "user",
                "invalid",
                "give exactly one of user and group",
            ),
        ])),
    }
}

/// Assigns a role to an account or a group globally or in projects (ACC-058…ACC-060).
#[utoipa::path(
    post,
    path = "/role-assignments",
    operation_id = "create_assignments",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    request_body = NewAssignment,
    responses(
        (status = CREATED, description = "Assignments made (existing ones are not repeated)", body = Assignments),
        (status = BAD_REQUEST, description = "Invalid scope or role", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions, named in the answer", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account or group", body = ErrorBody),
    ),
)]
pub async fn assign(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewAssignment>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let holder = holder(request.user_id, request.group_id)?;
    let mut scopes: Vec<Option<i64>> = request.projects.iter().copied().map(Some).collect();
    if request.global {
        scopes.push(None);
    }
    let made = roles::assign(&state.db, &viewer.access, request.role_id, holder, &scopes).await?;
    state.access.clear();
    let described = roles::describe(&state.db, made).await?;
    Ok(json(
        StatusCode::CREATED,
        Assignments {
            items: assignment_list(&state.names(), &viewer.access, &described),
        },
    ))
}

/// Revokes a direct assignment (ACC-059, ACC-060).
#[utoipa::path(
    delete,
    path = "/role-assignments/{id}",
    operation_id = "delete_assignment",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Assignment ID")),
    responses(
        (status = NO_CONTENT, description = "Revoked"),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such assignment", body = ErrorBody),
        (status = CONFLICT, description = "The last administrator", body = ErrorBody),
    ),
)]
pub async fn revoke(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    roles::revoke(&state.db, &viewer.access, id).await?;
    state.access.clear();
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn capability_key(capability: capability::Capability) -> CapabilityKey {
    match capability {
        capability::Capability::ShareViews => CapabilityKey::ShareViews,
    }
}

fn domain_capability(key: CapabilityKey) -> capability::Capability {
    match key {
        CapabilityKey::ShareViews => capability::Capability::ShareViews,
    }
}

fn capability_answer(
    item: &prostor_service::capabilities::CapabilityGroups,
    groups: &[prostor_store::groups::Group],
) -> Capability {
    Capability {
        key: capability_key(item.capability),
        groups: item
            .groups
            .iter()
            .filter_map(|id| groups.iter().find(|g| g.id == *id))
            .map(group_ref)
            .collect::<Vec<GroupRef>>(),
        can_be_switched_off: item.capability.can_be_switched_off(),
    }
}

/// Capabilities and their groups (ACC-064). Needs `system.read`.
#[utoipa::path(
    get,
    path = "/capabilities",
    operation_id = "list_capabilities",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    responses(
        (status = OK, description = "Every capability", body = Capabilities),
        (status = FORBIDDEN, description = "No `system.read`", body = ErrorBody),
    ),
)]
pub async fn capabilities(
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let list = prostor_service::capabilities::list(&state.db, &viewer.access).await?;
    let ids: Vec<i64> = list.iter().flat_map(|item| item.groups.clone()).collect();
    let groups = prostor_service::groups::refs(&state.db, &ids).await?;
    let items = list
        .iter()
        .map(|item| capability_answer(item, &groups))
        .collect();
    Ok(json(StatusCode::OK, Capabilities { items }))
}

/// Sets the groups a capability is on for (ACC-064). Needs `system.write`.
#[utoipa::path(
    put,
    path = "/capabilities/{key}",
    operation_id = "set_capability",
    tag = "roles",
    security(("session" = []), ("token" = [])),
    params(("key" = CapabilityKey, Path, description = "Capability")),
    request_body = CapabilityChange,
    responses(
        (status = OK, description = "The capability now", body = Capability),
        (status = BAD_REQUEST, description = "Unknown group, or the list may not be empty", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
    ),
)]
pub async fn set_capability(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(key): Path<CapabilityKey>,
    JsonBody(request): JsonBody<CapabilityChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let item = prostor_service::capabilities::set(
        &state.db,
        &viewer.access,
        domain_capability(key),
        &request.groups,
    )
    .await?;
    state.access.clear();
    let groups = prostor_service::groups::refs(&state.db, &item.groups).await?;
    Ok(json(StatusCode::OK, capability_answer(&item, &groups)))
}

/// The caller's effective permissions and capabilities (ACC-062). Works with every token.
#[utoipa::path(
    get,
    path = "/users/me/access",
    operation_id = "get_my_access",
    tag = "account",
    security(("session" = []), ("token" = [])),
    responses(
        (status = OK, description = "The caller's access", body = AccessMap),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn my_access(viewer: Viewer) -> Result<Response, ApiError> {
    let access = &viewer.access;
    let mut response = json(
        StatusCode::OK,
        AccessMap {
            global: permission_keys(access.global()),
            projects: access
                .projects()
                .map(|(project_id, set)| ProjectAccess {
                    project_id,
                    permissions: permission_keys(set),
                })
                .collect(),
            capabilities: access.capabilities().iter().map(capability_key).collect(),
        },
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}
