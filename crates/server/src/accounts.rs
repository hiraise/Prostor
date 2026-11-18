//! `/users`: the directory and people lookup, profiles, creating and inviting accounts, editing
//! identity, disabling, deleting, forcing a password change, and each person's roles
//! (ACC-002…ACC-018, ACC-061, AUTH-010, AUTH-014).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use taskmaster_api::access::{EffectiveRole, EffectiveRoles, RoleSource};
use taskmaster_api::accounts::{
    Account, AccountPatch, AccountState, AccountSummary, DirectoryQuery, DisableRequest, EmailSent,
    InvitationRequest, Invited, NewAccount, UserDeletion, UserDeletionImpact, UserSelection,
};
use taskmaster_api::error::ErrorBody;
use taskmaster_api::page::{Page, PageQuery};
use taskmaster_domain::UserId;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::auth::Scope;
use taskmaster_service::access::Source;
use taskmaster_service::mail::Letters;
use taskmaster_service::users::{self, IdentityInput, StateFilter};
use time::OffsetDateTime;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::{RequestInfo, require_any_scope, require_scope};
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::present::{self, account, account_summary, role_ref};

/// The largest page of the directory.
const DIRECTORY_MAX_LIMIT: u32 = 100;

/// How emails of this request are written: the base URL setting, else the page's origin.
pub fn letters(state: &AppState, info: &RequestInfo) -> Letters {
    let settings = state.settings.get();
    Letters {
        enabled: state.settings.mail().enabled,
        base_url: settings
            .get(&taskmaster_domain::settings::BASE_URL)
            .or_else(|| info.origin.clone()),
        language: settings.get(&taskmaster_domain::settings::SYSTEM_LANGUAGE),
    }
}

/// The directory and people lookup (ACC-002, ACC-070): accounts by name, filtered by text and
/// state. Needs `user.directory`.
#[utoipa::path(
    get,
    path = "/users",
    operation_id = "list_users",
    tag = "users",
    security(("session" = []), ("token" = [])),
    params(DirectoryQuery, PageQuery),
    responses(
        (status = OK, description = "A page of accounts", body = Page<AccountSummary>),
        (status = BAD_REQUEST, description = "Invalid limit or cursor", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `user.directory`", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<DirectoryQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_any_scope(&viewer.principal, &[Scope::TrackerRead, Scope::AdminRead])?;
    let page = query
        .resolve::<(String, UserId)>(DIRECTORY_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let state_filter = match filter.state.unwrap_or_default() {
        AccountState::All => StateFilter::All,
        AccountState::Enabled => StateFilter::Enabled,
        AccountState::Disabled => StateFilter::Disabled,
    };
    let rows = users::directory(
        &state.db,
        &viewer.access,
        filter.q.as_deref().unwrap_or_default(),
        state_filter,
        page.after.clone(),
        page.fetch_limit(),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |row| {
        (row.user.full_name.clone(), row.user.id)
    });
    Ok(json(
        StatusCode::OK,
        Page {
            items: page
                .items
                .iter()
                .map(|row| account_summary(&viewer.access, &row.user, &row.groups, &row.projects))
                .collect::<Vec<_>>(),
            next_cursor: page.next_cursor,
        },
    ))
}

/// Creates an account with a password (ACC-003). Needs `user.create` and `user.directory`.
#[utoipa::path(
    post,
    path = "/users",
    operation_id = "create_user",
    tag = "users",
    security(("session" = []), ("token" = [])),
    request_body = NewAccount,
    responses(
        (status = CREATED, description = "Created", body = Account),
        (status = BAD_REQUEST, description = "Invalid or taken values", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewAccount>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let user = users::create(
        &state.db,
        &state.passwords,
        &viewer.access,
        users::NewAccount {
            username: request.username,
            full_name: request.full_name,
            email: request.email,
            password: request.password,
            must_change_password: request.must_change_password,
        },
    )
    .await?;
    let profile = users::profile(&state.db, &viewer.access, user.id).await?;
    Ok(json(StatusCode::CREATED, account(&viewer.access, &profile)))
}

/// Invites people by email (ACC-004): their accounts are created at once and each gets a link to
/// choose a password. Needs outgoing email.
#[utoipa::path(
    post,
    path = "/users/invitations",
    operation_id = "invite_users",
    tag = "users",
    security(("session" = []), ("token" = [])),
    request_body = InvitationRequest,
    responses(
        (status = CREATED, description = "Accounts created and invitations queued", body = Invited),
        (status = BAD_REQUEST, description = "Invalid or taken addresses", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = CONFLICT, description = "Outgoing email is not configured", body = ErrorBody),
    ),
)]
pub async fn invite(
    State(state): State<AppState>,
    viewer: Viewer,
    info: RequestInfo,
    JsonBody(request): JsonBody<InvitationRequest>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let created = users::invite(
        &state.db,
        &viewer.access,
        &letters(&state, &info),
        &request.emails,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        Invited {
            items: created
                .iter()
                .map(|user| account_summary(&viewer.access, user, &[], &[]))
                .collect(),
        },
    ))
}

/// A profile (ACC-015): what the caller may see of it. The ID of a deleted account answers with
/// its replacement (ACC-013).
#[utoipa::path(
    get,
    path = "/users/{id}",
    operation_id = "get_user",
    tag = "users",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = OK, description = "The profile", body = Account),
        (status = NOT_FOUND, description = "No such account, or the caller may not see it", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_any_scope(&viewer.principal, &[Scope::TrackerRead, Scope::AdminRead])?;
    let profile = users::profile(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, account(&viewer.access, &profile)))
}

/// Changes name, username or email (ACC-009, ACC-017). One's own needs `user.self` (and a signed-in
/// session plus the current password to change the email); anyone else's `user.update`.
#[utoipa::path(
    patch,
    path = "/users/{id}",
    operation_id = "update_user",
    tag = "users",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    request_body = AccountPatch,
    responses(
        (status = OK, description = "Changed", body = Account),
        (status = BAD_REQUEST, description = "Invalid or taken values, wrong current password", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
        (status = CONFLICT, description = "Changed by someone else meanwhile", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    info: RequestInfo,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<AccountPatch>,
) -> Result<Response, ApiError> {
    let own = id == viewer.principal.user;
    if own {
        require_any_scope(&viewer.principal, &[Scope::TrackerWrite, Scope::AdminWrite])?;
        if request.email.is_some() && viewer.principal.session_id().is_none() {
            return Err(ApiError::new(
                taskmaster_api::error::ErrorCode::Forbidden,
                "Changing one's own email needs a signed-in session",
            ));
        }
    } else {
        require_scope(&viewer.principal, Scope::AdminWrite)?;
    }
    users::update_identity(
        &state.db,
        &state.passwords,
        &viewer.access,
        &letters(&state, &info),
        id,
        IdentityInput {
            username: request.username,
            full_name: request.full_name,
            email: request.email,
            current_password: request.current_password,
            version: request.version,
        },
        OffsetDateTime::now_utc(),
    )
    .await?;
    let profile = users::profile(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, account(&viewer.access, &profile)))
}

/// Disables accounts (ACC-005): sign-in, tokens and sessions stop at once.
#[utoipa::path(
    post,
    path = "/users/disable",
    operation_id = "disable_users",
    tag = "users",
    security(("session" = []), ("token" = [])),
    request_body = DisableRequest,
    responses(
        (status = NO_CONTENT, description = "Disabled"),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
        (status = CONFLICT, description = "One's own account, the guest or the last administrator", body = ErrorBody),
    ),
)]
pub async fn disable(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<DisableRequest>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    users::disable(
        &state.db,
        &viewer.access,
        &request.users,
        &request.reason,
        present::domain_label(request.label),
    )
    .await?;
    state.access.clear();
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Enables disabled accounts again (ACC-005).
#[utoipa::path(
    post,
    path = "/users/enable",
    operation_id = "enable_users",
    tag = "users",
    security(("session" = []), ("token" = [])),
    request_body = UserSelection,
    responses(
        (status = NO_CONTENT, description = "Enabled"),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
    ),
)]
pub async fn enable(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<UserSelection>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    users::enable(&state.db, &viewer.access, &request.users).await?;
    state.access.clear();
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// What deleting accounts would take away (ACC-006).
#[utoipa::path(
    post,
    path = "/users/deletion-impact",
    operation_id = "user_deletion_impact",
    tag = "users",
    security(("session" = []), ("token" = [])),
    request_body = UserSelection,
    responses(
        (status = OK, description = "Counts", body = UserDeletionImpact),
        (status = FORBIDDEN, description = "No `user.delete`", body = ErrorBody),
    ),
)]
pub async fn deletion_impact(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<UserSelection>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let impact = users::deletion_impact(&state.db, &viewer.access, &request.users).await?;
    Ok(json(
        StatusCode::OK,
        UserDeletionImpact {
            accounts: impact.accounts,
            sessions: impact.sessions,
            tokens: impact.tokens,
            memberships: impact.memberships,
            assignments: impact.assignments,
        },
    ))
}

/// Deletes accounts irreversibly (ACC-006); their IDs resolve to the replacement afterwards.
#[utoipa::path(
    post,
    path = "/users/delete",
    operation_id = "delete_users",
    tag = "users",
    security(("session" = []), ("token" = [])),
    request_body = UserDeletion,
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = BAD_REQUEST, description = "The replacement is among the deleted", body = ErrorBody),
        (status = FORBIDDEN, description = "No `user.delete`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
        (status = CONFLICT, description = "One's own account, the guest or the last administrator", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<UserDeletion>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    users::delete(
        &state.db,
        &viewer.access,
        &request.users,
        request.replacement,
    )
    .await?;
    state.access.clear();
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Makes the account choose a new password at its next sign-in (AUTH-010).
#[utoipa::path(
    post,
    path = "/users/{id}/password-change",
    operation_id = "require_password_change",
    tag = "users",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = NO_CONTENT, description = "Required"),
        (status = FORBIDDEN, description = "No `user.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
        (status = CONFLICT, description = "One's own account, or no password", body = ErrorBody),
    ),
)]
pub async fn require_password_change(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    users::require_password_change(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Sends a new invitation link to someone who has not joined yet (ACC-004).
#[utoipa::path(
    post,
    path = "/users/{id}/invitation",
    operation_id = "resend_invitation",
    tag = "users",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = NO_CONTENT, description = "Sent"),
        (status = FORBIDDEN, description = "No `user.create`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
        (status = CONFLICT, description = "Joined already, or no outgoing email", body = ErrorBody),
    ),
)]
pub async fn resend_invitation(
    State(state): State<AppState>,
    viewer: Viewer,
    info: RequestInfo,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    users::resend_invitation(
        &state.db,
        &viewer.access,
        &letters(&state, &info),
        id,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Sends a link that confirms the account's address (AUTH-014).
#[utoipa::path(
    post,
    path = "/users/{id}/email-verification",
    operation_id = "send_email_verification",
    tag = "users",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = NO_CONTENT, description = "Sent"),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
        (status = CONFLICT, description = "Verified already, no email, or no outgoing email", body = ErrorBody),
    ),
)]
pub async fn send_verification(
    State(state): State<AppState>,
    viewer: Viewer,
    info: RequestInfo,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    if id != viewer.principal.user {
        require_scope(&viewer.principal, Scope::AdminWrite)?;
    }
    users::request_verification(
        &state.db,
        &viewer.access,
        &letters(&state, &info),
        id,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Every role the account holds, with where each comes from (ACC-018, ACC-061). One's own are
/// always visible; anyone else's need `system.read`.
#[utoipa::path(
    get,
    path = "/users/{id}/roles",
    operation_id = "get_user_roles",
    tag = "users",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = OK, description = "Roles by scope", body = EffectiveRoles),
        (status = FORBIDDEN, description = "No `system.read`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
    ),
)]
pub async fn roles(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    if id != viewer.principal.user {
        require_scope(&viewer.principal, Scope::AdminRead)?;
        viewer
            .access
            .require(Permission::SystemRead)
            .map_err(taskmaster_service::Error::from)?;
    }
    let profile = users::profile(&state.db, &viewer.access, id).await?;
    let roles = taskmaster_service::access::effective_roles(&state.db, &profile.user).await?;
    Ok(json(StatusCode::OK, effective_roles(&roles)))
}

pub fn effective_roles(roles: &[taskmaster_service::access::EffectiveRole]) -> EffectiveRoles {
    EffectiveRoles {
        items: roles
            .iter()
            .map(|role| EffectiveRole {
                role: role_ref(&role.role),
                project_id: role.scope.project(),
                permissions: present::permission_keys(role.scope.applied(
                    taskmaster_domain::access::role::role_permissions(
                        role.role.builtin.as_deref(),
                        &role.role.permissions,
                    ),
                )),
                sources: role
                    .sources
                    .iter()
                    .map(|source| match source {
                        Source::Direct { assignment } => RoleSource::Direct {
                            assignment_id: *assignment,
                        },
                        Source::Group {
                            id,
                            name,
                            builtin,
                            team_of,
                        } => RoleSource::Group {
                            group: taskmaster_api::access::GroupRef {
                                id: *id,
                                name: name.clone(),
                                builtin: present::builtin_group(builtin.map(|group| group.key())),
                                team_of: *team_of,
                            },
                        },
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Sends a test message to the caller's own address (ACC-017, NTF-007).
#[utoipa::path(
    post,
    path = "/users/me/test-email",
    operation_id = "send_test_email",
    tag = "account",
    security(("session" = [])),
    responses(
        (status = OK, description = "Queued", body = EmailSent),
        (status = CONFLICT, description = "No email, or no outgoing email", body = ErrorBody),
    ),
)]
pub async fn test_email(
    State(state): State<AppState>,
    viewer: Viewer,
    info: RequestInfo,
) -> Result<Response, ApiError> {
    if viewer.principal.session_id().is_none() {
        return Err(ApiError::new(
            taskmaster_api::error::ErrorCode::Forbidden,
            "This needs a signed-in session",
        ));
    }
    let email =
        users::send_test_message(&state.db, &viewer.access, &letters(&state, &info)).await?;
    Ok(json(StatusCode::OK, EmailSent { email }))
}
