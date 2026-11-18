//! `/setup`: the first-run setup wizard (ADM-047).

use std::sync::atomic::Ordering;
use std::time::Instant;

use axum::extract::State;
use axum::response::Response;
use taskmaster_api::auth::Session;
use taskmaster_api::error::ErrorBody;
use taskmaster_api::setup::{Language, SetupRequest};
use taskmaster_domain::validation::Problem;
use taskmaster_service::setup::Setup;
use taskmaster_store::PgPool;
use time::OffsetDateTime;

use crate::AppState;
use crate::auth::RequestInfo;
use crate::error::{ApiError, JsonBody};
use crate::session::signed_in_response;
use crate::throttle::Key;

/// Creates the administrator, sets the language, address and time zone, and signs the
/// administrator in. Works once, with the setup code from the server log.
#[utoipa::path(
    post,
    path = "/setup",
    operation_id = "run_setup",
    tag = "setup",
    request_body = SetupRequest,
    responses(
        (status = CREATED, description = "Set up; the administrator is signed in", body = Session),
        (status = BAD_REQUEST, description = "Invalid values or a wrong setup code", body = ErrorBody),
        (status = FORBIDDEN, description = "Sent from another site", body = ErrorBody),
        (status = CONFLICT, description = "The setup is already complete", body = ErrorBody),
        (status = TOO_MANY_REQUESTS, description = "Too many wrong codes", body = ErrorBody),
    ),
)]
pub async fn run(
    State(state): State<AppState>,
    info: RequestInfo,
    JsonBody(request): JsonBody<SetupRequest>,
) -> Result<Response, ApiError> {
    info.require_same_origin()?;
    let keys = [Key::Setup(info.ip)];
    state
        .throttle
        .check(&keys, Instant::now())
        .map_err(ApiError::rate_limited)?;

    let setup = Setup {
        code: request.code,
        username: request.username,
        full_name: request.full_name,
        email: request.email,
        password: request.password,
        system_language: match request.system_language {
            Language::En => "en",
            Language::Ru => "ru",
        }
        .to_owned(),
        base_url: request.base_url,
        time_zone: request.time_zone,
    };
    let result = taskmaster_service::setup::run(
        &state.db,
        &state.passwords,
        setup,
        OffsetDateTime::now_utc(),
        &info.origin(),
    )
    .await;
    let signed_in = match result {
        Ok(signed_in) => signed_in,
        Err(error) => {
            if let taskmaster_service::Error::Invalid(problems) = &error
                && problems
                    .iter()
                    .any(|p| p.path == "code" && p.problem == Problem::Incorrect)
            {
                state.throttle.failed(&keys, Instant::now());
            }
            return Err(error.into());
        }
    };
    state.setup_done.store(true, Ordering::Release);
    // The cookie's `Secure` flag follows the base URL that was just chosen.
    state.settings.reload(&state.db).await?;
    Ok(signed_in_response(&state, signed_in))
}

/// Whether the setup still has to run. Remembered once complete: it never becomes pending again.
pub async fn required(state: &AppState) -> Result<bool, sqlx::Error> {
    if state.setup_done.load(Ordering::Acquire) {
        return Ok(false);
    }
    let complete = complete(&state.db).await?;
    if complete {
        state.setup_done.store(true, Ordering::Release);
    }
    Ok(!complete)
}

async fn complete(pool: &PgPool) -> Result<bool, sqlx::Error> {
    taskmaster_store::instance::setup_complete(pool).await
}
