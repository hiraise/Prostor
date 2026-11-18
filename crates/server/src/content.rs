//! `/markdown`: the preview of rich text (CNT-029, CNT-038). Rendering lives in the crate
//! `markup`; the flow in `prostor_service::text`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use prostor_api::content::{MarkdownPreview, MarkdownProfile, Rendered};
use prostor_api::error::ErrorBody;
use prostor_domain::auth::Scope;
use prostor_markup::Profile;
use prostor_service::text;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, json};
use crate::present::text_refs;

/// Renders a text as it would be stored, with what its markers stand for for the caller.
#[utoipa::path(
    post,
    path = "/markdown",
    operation_id = "preview_markdown",
    tag = "content",
    security(("session" = []), ("token" = [])),
    request_body = MarkdownPreview,
    responses(
        (status = OK, description = "The rendered text", body = Rendered),
        (status = BAD_REQUEST, description = "The text is too large", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn preview(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<MarkdownPreview>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let profile = match request.profile {
        MarkdownProfile::Full => Profile::Full,
        MarkdownProfile::Basic => Profile::Basic,
    };
    let (rendered, refs) = text::preview(
        &state.db,
        &viewer.access,
        &request.text,
        profile,
        request.issue,
        &request.uploads,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        Rendered {
            refs: text_refs(
                &state.names(),
                &viewer.access,
                &state.signer(),
                &refs,
                &rendered.users,
                &rendered.issue_tokens,
                &rendered.files,
            ),
            html: rendered.html,
        },
    ))
}
