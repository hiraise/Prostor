//! The admin mail settings (NTF-002, NTF-004, NTF-006, NTF-007; plan step 1.10 §5.1): the screen
//! of «Администрирование» → «Почта», the pattern of the link types. Reading needs `system.read`,
//! saving and the test send need `system.write`; secret values are never part of an answer.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use trackflow_api::admin::{MailSettings, MailSettingsChange, MailSource, MailTestSend};
use trackflow_api::error::ErrorBody;
use trackflow_domain::auth::Scope;
use trackflow_domain::settings::{BASE_URL, SYSTEM_LANGUAGE};
use trackflow_service::mail::{self, MailSettingsChange as Change};

use crate::AppState;
use crate::access::Viewer;
use crate::auth::{RequestInfo, require_scope};
use crate::error::{ApiError, JsonBody, json};

/// The mail settings of the instance, without any secret value (NTF-004).
#[utoipa::path(
    get,
    path = "/admin/mail",
    operation_id = "get_mail_settings",
    tag = "admin",
    security(("session" = []), ("token" = [])),
    responses(
        (status = OK, description = "The mail settings", body = MailSettings),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.read`", body = ErrorBody),
    ),
)]
pub async fn settings(State(state): State<AppState>, viewer: Viewer) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let view = mail::mail_settings(
        &state.db,
        &viewer.access,
        state.settings.secret_key(),
        state.settings.env_mail(),
    )
    .await?;
    Ok(json(StatusCode::OK, shape(view)))
}

/// Saves the mail settings (NTF-004, NTF-006): validates the whole set by building the transport
/// it names, then stores each change with its audit event. Switching email on (NTF-002) needs a
/// working transport. Needs `system.write`; storing a secret needs the instance key.
#[utoipa::path(
    put,
    path = "/admin/mail",
    operation_id = "save_mail_settings",
    tag = "admin",
    security(("session" = []), ("token" = [])),
    request_body = MailSettingsChange,
    responses(
        (status = OK, description = "Saved; the settings as they stand now", body = MailSettings),
        (status = BAD_REQUEST, description = "The set names no working transport", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`, or no key to store secrets", body = ErrorBody),
    ),
)]
pub async fn save(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<MailSettingsChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let view = mail::save_mail_settings(
        &state.db,
        &viewer.access,
        &Change {
            smtp_url: request.smtp_url,
            smtp_username: request.smtp_username,
            smtp_password: request.smtp_password,
            mail_from: request.mail_from,
            mail_reply_to: request.mail_reply_to,
            email_enabled: request.email_enabled,
        },
        state.settings.secret_key(),
        state.settings.env_mail(),
    )
    .await?;
    // This node's mail state follows at once; other nodes follow the settings bus.
    state.settings.reload(&state.db).await?;
    Ok(json(StatusCode::OK, shape(view)))
}

/// Sends a test message to any address through the saved transport and answers the transport's
/// complaint in full (NTF-007). The switch does not gate it: an administrator tests before
/// switching email on. Needs `system.write`.
#[utoipa::path(
    post,
    path = "/admin/mail/test",
    operation_id = "send_admin_test_email",
    tag = "admin",
    security(("session" = []), ("token" = [])),
    request_body = MailTestSend,
    responses(
        (status = OK, description = "Sent", body = MailTestSend),
        (status = BAD_REQUEST, description = "Not an address", body = ErrorBody),
        (status = CONFLICT, description = "The transport's complaint, in full", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
    ),
)]
pub async fn test(
    State(state): State<AppState>,
    viewer: Viewer,
    info: RequestInfo,
    JsonBody(request): JsonBody<MailTestSend>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let outgoing = state.settings.mail();
    let Some(mailer) = outgoing.transport.clone() else {
        return Err(ApiError::new(
            trackflow_api::error::ErrorCode::Conflict,
            "no mail transport: configure the server and the sender first",
        ));
    };
    let settings = state.settings.get();
    let base = settings
        .get(&BASE_URL)
        .or_else(|| info.origin.clone())
        .unwrap_or_default();
    mail::send_admin_test(
        &viewer.access,
        &request.to,
        &base,
        settings.get(&SYSTEM_LANGUAGE),
        &mailer,
    )
    .await?;
    Ok(json(StatusCode::OK, request))
}

/// The service view as the API shape: the same fields, no secrets on either side.
fn shape(view: mail::MailSettingsView) -> MailSettings {
    MailSettings {
        source: match view.source {
            mail::MailSource::Settings => MailSource::Settings,
            mail::MailSource::Env => MailSource::Env,
            mail::MailSource::None => MailSource::None,
        },
        configured: view.configured,
        email_enabled: view.email_enabled,
        mail_from: view.mail_from,
        mail_reply_to: view.mail_reply_to,
        has_smtp_url: view.has_smtp_url,
        has_username: view.has_username,
        has_password: view.has_password,
    }
}
