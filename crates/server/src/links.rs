//! Emailed one-time links, used without signing in: asking for a password reset, choosing the new
//! password, accepting an invitation, confirming an address (AUTH-008, AUTH-014, ACC-004). The
//! secret travels in the body, never in the address, so it stays out of logs.

use std::time::Instant;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use time::OffsetDateTime;
use trackflow_api::accounts::{
    EmailSent, InvitationAcceptance, LinkInfo, LinkPurpose, LinkToken, PasswordResetConfirm,
    PasswordResetRequest,
};
use trackflow_api::auth::Session;
use trackflow_api::error::ErrorBody;
use trackflow_domain::auth;
use trackflow_service::links::{self, Acceptance};

use crate::AppState;
use crate::accounts::letters;
use crate::auth::RequestInfo;
use crate::error::{ApiError, JsonBody, json};
use crate::session::signed_in_response;
use crate::throttle::Key;

/// Asks for a password-reset link (AUTH-008). The answer is the same whether or not an account has
/// the address; the link goes only to a verified address, at most once a minute.
#[utoipa::path(
    post,
    path = "/password-reset",
    operation_id = "request_password_reset",
    tag = "session",
    request_body = PasswordResetRequest,
    responses(
        (status = ACCEPTED, description = "If an account has this verified address, a link is on its way"),
        (status = BAD_REQUEST, description = "No address", body = ErrorBody),
        (status = FORBIDDEN, description = "Sent from another site", body = ErrorBody),
        (status = CONFLICT, description = "Outgoing email is not configured", body = ErrorBody),
        (status = TOO_MANY_REQUESTS, description = "Too many requests", body = ErrorBody),
    ),
)]
pub async fn request_reset(
    State(state): State<AppState>,
    info: RequestInfo,
    JsonBody(request): JsonBody<PasswordResetRequest>,
) -> Result<Response, ApiError> {
    info.require_same_origin()?;
    // Every request counts, so nobody floods a mailbox or probes addresses quickly.
    let keys = [Key::login(&request.email, info.ip), Key::Address(info.ip)];
    state
        .throttle
        .check(&keys, Instant::now())
        .map_err(ApiError::rate_limited)?;
    state.throttle.failed(&keys, Instant::now());
    links::request_password_reset(
        &state.db,
        &letters(&state, &info),
        &request.email,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(StatusCode::ACCEPTED.into_response())
}

/// What a link is for, before it is used: the page shows whose it is or that it expired.
#[utoipa::path(
    post,
    path = "/account-links/inspect",
    operation_id = "inspect_account_link",
    tag = "session",
    request_body = LinkToken,
    responses(
        (status = OK, description = "The link", body = LinkInfo),
        (status = NOT_FOUND, description = "Unknown or used link", body = ErrorBody),
    ),
)]
pub async fn inspect(
    State(state): State<AppState>,
    JsonBody(request): JsonBody<LinkToken>,
) -> Result<Response, ApiError> {
    let link = links::inspect(&state.db, &request.token, OffsetDateTime::now_utc()).await?;
    let mut response = json(
        StatusCode::OK,
        LinkInfo {
            purpose: match link.purpose {
                auth::LinkPurpose::PasswordReset => LinkPurpose::PasswordReset,
                auth::LinkPurpose::EmailVerification => LinkPurpose::EmailVerification,
                auth::LinkPurpose::Invitation => LinkPurpose::Invitation,
            },
            email: link.email,
            username: link.username,
            full_name: link.full_name,
            expired: link.expired,
        },
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// Sets a new password from a reset link and signs in; every other session ends (AUTH-008).
#[utoipa::path(
    post,
    path = "/password-reset/confirm",
    operation_id = "confirm_password_reset",
    tag = "session",
    request_body = PasswordResetConfirm,
    responses(
        (status = CREATED, description = "Signed in with the new password", body = Session),
        (status = BAD_REQUEST, description = "Invalid password", body = ErrorBody),
        (status = FORBIDDEN, description = "Sent from another site", body = ErrorBody),
        (status = NOT_FOUND, description = "Unknown or used link", body = ErrorBody),
        (status = CONFLICT, description = "The link has expired", body = ErrorBody),
    ),
)]
pub async fn confirm_reset(
    State(state): State<AppState>,
    info: RequestInfo,
    JsonBody(request): JsonBody<PasswordResetConfirm>,
) -> Result<Response, ApiError> {
    info.require_same_origin()?;
    let signed_in = links::reset_password(
        &state.db,
        &state.passwords,
        &request.token,
        &request.new_password,
        OffsetDateTime::now_utc(),
        &info.origin(),
    )
    .await?;
    Ok(signed_in_response(&state, signed_in))
}

/// Joins from an invitation: chooses a password (and optionally a name and username), verifies
/// the address and signs in (ACC-004).
#[utoipa::path(
    post,
    path = "/invitations/accept",
    operation_id = "accept_invitation",
    tag = "session",
    request_body = InvitationAcceptance,
    responses(
        (status = CREATED, description = "Signed in", body = Session),
        (status = BAD_REQUEST, description = "Invalid or taken values", body = ErrorBody),
        (status = FORBIDDEN, description = "Sent from another site", body = ErrorBody),
        (status = NOT_FOUND, description = "Unknown or used link", body = ErrorBody),
        (status = CONFLICT, description = "The link has expired", body = ErrorBody),
    ),
)]
pub async fn accept_invitation(
    State(state): State<AppState>,
    info: RequestInfo,
    JsonBody(request): JsonBody<InvitationAcceptance>,
) -> Result<Response, ApiError> {
    info.require_same_origin()?;
    let signed_in = links::accept_invitation(
        &state.db,
        &state.passwords,
        &request.token,
        Acceptance {
            password: request.password,
            full_name: request.full_name,
            username: request.username,
        },
        OffsetDateTime::now_utc(),
        &info.origin(),
    )
    .await?;
    Ok(signed_in_response(&state, signed_in))
}

/// Confirms an address from a verification link (AUTH-014).
#[utoipa::path(
    post,
    path = "/email-verification/confirm",
    operation_id = "confirm_email",
    tag = "session",
    request_body = LinkToken,
    responses(
        (status = OK, description = "Confirmed", body = EmailSent),
        (status = NOT_FOUND, description = "Unknown or used link, or the account has another address now", body = ErrorBody),
        (status = CONFLICT, description = "The link has expired", body = ErrorBody),
    ),
)]
pub async fn confirm_email(
    State(state): State<AppState>,
    info: RequestInfo,
    JsonBody(request): JsonBody<LinkToken>,
) -> Result<Response, ApiError> {
    info.require_same_origin()?;
    let email = links::confirm_email(&state.db, &request.token, OffsetDateTime::now_utc()).await?;
    Ok(json(StatusCode::OK, EmailSent { email }))
}
