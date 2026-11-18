//! `/users/me`: the caller's account, password, personal API tokens and interface settings
//! (AUTH-009, AUTH-029, AUTH-030, UI-018).

use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use prostor_api::auth::{
    ApiToken, ChangePasswordRequest, CreatedApiToken, CurrentUser, NewApiToken, TokenScope,
};
use prostor_api::error::ErrorBody;
use prostor_api::page::{Page, PageQuery};
use prostor_api::preferences as wire;
use prostor_domain::auth::Scope;
use prostor_domain::preferences as prefs;
use prostor_domain::validation::Problem;
use prostor_service::tokens::NewTokenInput;
use prostor_store::api_tokens;

use crate::AppState;
use crate::auth::{Caller, PendingSession, RequestInfo, SessionCaller};
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::session::current_user;
use crate::throttle::Key;

/// The largest page of tokens.
const TOKENS_MAX_LIMIT: u32 = 100;

/// The caller's own account. Works with every API token, whatever its scopes.
#[utoipa::path(
    get,
    path = "/users/me",
    operation_id = "get_me",
    tag = "account",
    security(("session" = []), ("token" = [])),
    responses(
        (status = OK, description = "The caller's account", body = CurrentUser),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn me(
    State(state): State<AppState>,
    Caller(principal): Caller,
) -> Result<Response, ApiError> {
    let user = prostor_service::account::me(&state.db, principal.user).await?;
    Ok(json(StatusCode::OK, current_user(&user)))
}

/// Changes the caller's password. Every other session of the account ends; API tokens keep
/// working. Also the way out of a required change (AUTH-010).
#[utoipa::path(
    post,
    path = "/users/me/password",
    operation_id = "change_password",
    tag = "account",
    security(("session" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = NO_CONTENT, description = "Changed"),
        (status = BAD_REQUEST, description = "Wrong current password or invalid new one", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
        (status = CONFLICT, description = "Changed by another request at the same time", body = ErrorBody),
        (status = TOO_MANY_REQUESTS, description = "Too many wrong current passwords", body = ErrorBody),
    ),
)]
pub async fn change_password(
    State(state): State<AppState>,
    PendingSession(caller): PendingSession,
    info: RequestInfo,
    JsonBody(request): JsonBody<ChangePasswordRequest>,
) -> Result<Response, ApiError> {
    // A taken-over session must not become a way to guess the password.
    let keys = [Key::login(&format!("#{}", caller.principal.user), info.ip)];
    state
        .throttle
        .check(&keys, Instant::now())
        .map_err(ApiError::rate_limited)?;
    let result = prostor_service::account::change_password(
        &state.db,
        &state.passwords,
        &caller.principal,
        &request.current_password,
        &request.new_password,
    )
    .await;
    if let Err(prostor_service::Error::Invalid(problems)) = &result
        && problems
            .iter()
            .any(|p| p.path == "current_password" && p.problem == Problem::Incorrect)
    {
        state.throttle.failed(&keys, Instant::now());
    }
    result?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The caller's personal API tokens, newest first.
#[utoipa::path(
    get,
    path = "/users/me/tokens",
    operation_id = "list_my_tokens",
    tag = "account",
    security(("session" = [])),
    params(PageQuery),
    responses(
        (status = OK, description = "A page of tokens", body = Page<ApiToken>),
        (status = BAD_REQUEST, description = "Invalid limit or cursor", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
    ),
)]
pub async fn list_tokens(
    State(state): State<AppState>,
    caller: SessionCaller,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    let page = query
        .resolve::<i64>(TOKENS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let rows = prostor_service::tokens::list(
        &state.db,
        caller.principal.user,
        page.after,
        page.fetch_limit(),
    )
    .await?;
    let tokens: Vec<ApiToken> = rows.iter().map(api_token).collect();
    Ok(json(
        StatusCode::OK,
        Page::from_rows(tokens, page.limit, |token| token.id),
    ))
}

/// Creates a personal API token. The answer holds its secret, which is never shown again.
#[utoipa::path(
    post,
    path = "/users/me/tokens",
    operation_id = "create_my_token",
    tag = "account",
    security(("session" = [])),
    request_body = NewApiToken,
    responses(
        (status = CREATED, description = "Created", body = CreatedApiToken),
        (status = BAD_REQUEST, description = "Invalid name or scopes", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
    ),
)]
pub async fn create_token(
    State(state): State<AppState>,
    caller: SessionCaller,
    JsonBody(request): JsonBody<NewApiToken>,
) -> Result<Response, ApiError> {
    let input = NewTokenInput {
        name: request.name,
        scopes: request
            .scopes
            .iter()
            .map(|scope| domain_scope(*scope).key().to_owned())
            .collect(),
    };
    let created = prostor_service::tokens::create(&state.db, &caller.principal, input).await?;
    let mut response = json(
        StatusCode::CREATED,
        CreatedApiToken {
            token: api_token(&created.token),
            secret: created.secret.value,
        },
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

/// Deletes one of the caller's tokens; it stops working at once.
#[utoipa::path(
    delete,
    path = "/users/me/tokens/{id}",
    operation_id = "delete_my_token",
    tag = "account",
    security(("session" = [])),
    params(("id" = i64, Path, description = "Token ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
        (status = NOT_FOUND, description = "The caller has no such token", body = ErrorBody),
    ),
)]
pub async fn delete_token(
    State(state): State<AppState>,
    caller: SessionCaller,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    prostor_service::tokens::delete(&state.db, &caller.principal, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn api_token(token: &api_tokens::ApiToken) -> ApiToken {
    ApiToken {
        id: token.id,
        name: token.name.clone(),
        scopes: prostor_domain::auth::Scopes::from_stored(&token.scopes)
            .iter()
            .map(api_scope)
            .collect(),
        created_by: token.created_by,
        created_at: token.created_at,
        last_used_at: token.last_used_at,
        version: token.version,
    }
}

fn domain_scope(scope: TokenScope) -> Scope {
    match scope {
        TokenScope::TrackerRead => Scope::TrackerRead,
        TokenScope::TrackerWrite => Scope::TrackerWrite,
        TokenScope::AdminRead => Scope::AdminRead,
        TokenScope::AdminWrite => Scope::AdminWrite,
    }
}

fn api_scope(scope: Scope) -> TokenScope {
    match scope {
        Scope::TrackerRead => TokenScope::TrackerRead,
        Scope::TrackerWrite => TokenScope::TrackerWrite,
        Scope::AdminRead => TokenScope::AdminRead,
        Scope::AdminWrite => TokenScope::AdminWrite,
    }
}

/// Enums of the settings have the same variants on the wire and in the domain.
macro_rules! same_enum {
    ($wire:ty, $domain:ty, [$($variant:ident),*]) => {
        impl From<$wire> for Wrap<$domain> {
            fn from(value: $wire) -> Self {
                Wrap(match value { $(<$wire>::$variant => <$domain>::$variant,)* })
            }
        }
        impl From<$domain> for Wrap<$wire> {
            fn from(value: $domain) -> Self {
                Wrap(match value { $(<$domain>::$variant => <$wire>::$variant,)* })
            }
        }
    };
}

/// A local wrapper, so conversions between two foreign types can be written here.
struct Wrap<T>(T);

macro_rules! same_type {
    ($($ty:ty),*) => {
        $(impl From<$ty> for Wrap<$ty> {
            fn from(value: $ty) -> Self {
                Wrap(value)
            }
        })*
    };
}

same_type!(bool, i32, i64, String);

impl<A, B> From<Vec<A>> for Wrap<Vec<B>>
where
    Wrap<B>: From<A>,
{
    fn from(items: Vec<A>) -> Self {
        Wrap(items.into_iter().map(|a| Wrap::<B>::from(a).0).collect())
    }
}

same_enum!(wire::HomePage, prefs::Home, [Issues, Projects, Boards]);
same_enum!(wire::PanelSide, prefs::PanelSide, [Start, End]);
same_enum!(
    wire::ActivityOrder,
    prefs::ActivityOrder,
    [OldestFirst, NewestFirst]
);
same_enum!(wire::LongComments, prefs::LongComments, [Full, Collapsed]);
same_enum!(
    wire::ListSidebarMode,
    prefs::ListSidebar,
    [Open, Auto, Closed]
);
same_enum!(
    wire::SidebarSection,
    prefs::SidebarSection,
    [Drafts, Projects, Tags, Searches]
);
same_enum!(
    wire::CommandSilentPrompt,
    prefs::CommandSilentPrompt,
    [Silent, Dismissed, Never]
);

/// Moves every key between the wire's and the domain's settings.
macro_rules! convert {
    ($($key:ident),* $(,)?) => {
        fn preferences_out(p: prefs::Preferences) -> wire::Preferences {
            wire::Preferences { $($key: p.$key.map(|v| Wrap::from(v).0),)* }
        }
        fn patch_in(p: wire::PreferencesPatch) -> prefs::Patch {
            prefs::Patch { $($key: p.$key.map(|v| v.map(|v| Wrap::from(v).0)),)* }
        }
    };
}

convert!(
    rail_collapsed,
    rail_width,
    editor_toolbar,
    home,
    issue_panel_side,
    issue_panel_width,
    issue_panel_collapsed,
    activity_order,
    activity_comments,
    activity_changes,
    long_comments,
    attachments_collapsed,
    links_collapsed,
    list_hide_resolved,
    list_columns,
    list_sidebar,
    list_sidebar_width,
    list_sidebar_folded,
    recent_issues,
    last_project,
    commands_on_typing,
    commands_silent_prompt,
);

/// The caller's interface settings (UI-018): keys they never set are absent.
#[utoipa::path(
    get,
    path = "/users/me/preferences",
    operation_id = "get_my_preferences",
    tag = "account",
    security(("session" = [])),
    responses(
        (status = OK, description = "The caller's settings", body = wire::Preferences),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
    ),
)]
pub async fn preferences(
    State(state): State<AppState>,
    caller: SessionCaller,
) -> Result<Response, ApiError> {
    let found = prostor_service::preferences::get(&state.db, caller.principal.user).await?;
    Ok(json(StatusCode::OK, preferences_out(found)))
}

/// Changes the caller's interface settings (UI-018): given keys are set, `null` ones reset, the
/// rest stay. Answers all of them.
#[utoipa::path(
    patch,
    path = "/users/me/preferences",
    operation_id = "update_my_preferences",
    tag = "account",
    security(("session" = [])),
    request_body = wire::PreferencesPatch,
    responses(
        (status = OK, description = "The caller's settings", body = wire::Preferences),
        (status = BAD_REQUEST, description = "A value out of range, or too much in all", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
    ),
)]
pub async fn update_preferences(
    State(state): State<AppState>,
    caller: SessionCaller,
    JsonBody(patch): JsonBody<wire::PreferencesPatch>,
) -> Result<Response, ApiError> {
    let saved =
        prostor_service::preferences::update(&state.db, caller.principal.user, patch_in(patch))
            .await?;
    Ok(json(StatusCode::OK, preferences_out(saved)))
}
