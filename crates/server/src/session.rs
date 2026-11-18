//! `/session`: signing in with a password, the current session and signing out (AUTH-001,
//! AUTH-003, AUTH-005).

use std::time::Instant;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use prostor_api::auth::{CurrentUser, Session, SignInRequest};
use prostor_api::error::ErrorBody;
use prostor_domain::auth::cookie_lifetime;
use prostor_domain::settings::session_policy;
use prostor_service::auth::{SessionInfo, SignIn, SignedIn};
use prostor_store::users::User;
use time::OffsetDateTime;

use crate::AppState;
use crate::auth::{PendingSession, RequestInfo, clear_session_cookie, session_cookie};
use crate::error::{ApiError, JsonBody, json};
use crate::throttle::Key;

/// Signs in with a username or email and a password and starts a browser session, set as an
/// HTTP-only cookie.
#[utoipa::path(
    post,
    path = "/session",
    operation_id = "sign_in",
    tag = "session",
    request_body = SignInRequest,
    responses(
        (status = CREATED, description = "Signed in; the session cookie is set", body = Session),
        (status = BAD_REQUEST, description = "Login or password missing", body = ErrorBody),
        (status = UNAUTHORIZED, description = "`invalid_credentials`", body = ErrorBody),
        (status = FORBIDDEN, description = "Sent from another site", body = ErrorBody),
        (status = TOO_MANY_REQUESTS, description = "Too many failed attempts", body = ErrorBody),
    ),
)]
pub async fn sign_in(
    State(state): State<AppState>,
    info: RequestInfo,
    JsonBody(request): JsonBody<SignInRequest>,
) -> Result<Response, ApiError> {
    info.require_same_origin()?;
    let login_key = Key::login(&request.login, info.ip);
    let keys = [login_key.clone(), Key::Address(info.ip)];
    state
        .throttle
        .check(&keys, Instant::now())
        .map_err(ApiError::rate_limited)?;

    let policy = session_policy(&state.settings.get());
    let attempt = SignIn {
        login: request.login,
        password: request.password,
        remember: request.remember,
    };
    let signed_in = match prostor_service::auth::sign_in(
        &state.db,
        &state.passwords,
        attempt,
        policy,
        OffsetDateTime::now_utc(),
        &info.origin(),
    )
    .await
    {
        Ok(signed_in) => signed_in,
        Err(error) => {
            if matches!(error, prostor_service::Error::InvalidCredentials) {
                state.throttle.failed(&keys, Instant::now());
            }
            return Err(error.into());
        }
    };
    state.throttle.succeeded(&login_key);
    Ok(signed_in_response(&state, signed_in))
}

/// The answer to a sign-in: the session and its cookie.
pub fn signed_in_response(state: &AppState, signed_in: SignedIn) -> Response {
    let policy = session_policy(&state.settings.get());
    let cookie = session_cookie(
        state,
        &signed_in.secret.value,
        cookie_lifetime(policy, signed_in.session.remember),
    );
    let mut response = json(
        StatusCode::CREATED,
        session_body(&signed_in.user, &signed_in.session),
    );
    no_store(&mut response);
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
}

/// The current browser session and its user.
#[utoipa::path(
    get,
    path = "/session",
    operation_id = "get_session",
    tag = "session",
    security(("session" = [])),
    responses(
        (status = OK, description = "Signed in", body = Session),
        (status = UNAUTHORIZED, description = "Not signed in, or the session has ended", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    PendingSession(caller): PendingSession,
) -> Result<Response, ApiError> {
    let session = prostor_service::auth::session(&state.db, caller.session).await?;
    let user = prostor_service::account::me(&state.db, caller.principal.user).await?;
    let mut response = json(StatusCode::OK, session_body(&user, &session));
    no_store(&mut response);
    Ok(response)
}

/// Signs out: ends the session and removes its cookie. Other tabs of the browser notice on their
/// next request.
#[utoipa::path(
    delete,
    path = "/session",
    operation_id = "sign_out",
    tag = "session",
    responses(
        (status = NO_CONTENT, description = "Signed out (also when already signed out)"),
        (status = FORBIDDEN, description = "Sent from another site", body = ErrorBody),
    ),
)]
pub async fn sign_out(
    State(state): State<AppState>,
    info: RequestInfo,
) -> Result<Response, ApiError> {
    info.require_same_origin()?;
    if let Some(cookie) = &info.session_cookie {
        prostor_service::auth::sign_out_by_cookie(&state.db, cookie).await?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, clear_session_cookie(&state));
    Ok(response)
}

pub fn current_user(user: &User) -> CurrentUser {
    CurrentUser {
        id: user.id,
        username: user.username.clone(),
        full_name: user.full_name.clone(),
        email: user.email.clone(),
        email_verified: user.email_verified_at.is_some(),
        avatar_color: user.avatar_color,
        avatar_url: user
            .avatar_key
            .as_deref()
            .map(|key| crate::present::avatar_url(user.id, key)),
        version: user.version,
    }
}

fn session_body(user: &User, session: &SessionInfo) -> Session {
    Session {
        user: current_user(user),
        remember: session.remember,
        created_at: session.created_at,
        password_change_required: session.must_change_password,
    }
}

/// Answers about sessions are personal: no cache may keep them.
fn no_store(response: &mut Response) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}
