//! The OpenAPI document of `/api/v1` (D-006). Paths come from the `#[utoipa::path]` annotations of
//! the handlers registered in `lib.rs`; the web app's TypeScript types are generated from it
//! (`make api`).

use prostor_api::error::{ErrorBody, ErrorCode, FieldError};
use prostor_api::live::{LiveClientMessage, LiveServerMessage};
use utoipa::openapi::security::{ApiKey, ApiKeyValue, Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::auth::SESSION_COOKIE;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Prostor API",
        description = "The API of Prostor, used by its web app, scripts and integrations. \
                       Conventions: snake_case JSON, one error format, cursor pagination."
    ),
    servers((url = "/api/v1")),
    // Live messages travel over the WebSocket of step 1.6, not over a documented path.
    components(schemas(ErrorBody, ErrorCode, FieldError, LiveClientMessage, LiveServerMessage)),
    modifiers(&Credentials),
    tags(
        (name = "health", description = "Liveness and readiness for orchestrators"),
        (name = "bootstrap", description = "Configuration clients load before rendering"),
        (name = "setup", description = "The first-run setup wizard"),
        (name = "session", description = "Signing in and out of the web app"),
        (name = "account", description = "The caller's account, password and API tokens"),
        (name = "projects", description = "Projects, their teams and people"),
        (name = "fields", description = "Custom fields, value sets and the fields of projects"),
        (name = "issues", description = "Issues and drafts, their fields, activity and tags"),
        (name = "comments", description = "Comments on issues"),
        (name = "links", description = "Links between issues and link types"),
        (name = "admin", description = "Instance administration: the mail settings"),
        (name = "boards", description = "Boards, their columns and sprints, and the board view"),
        (name = "notifications", description = "Notification settings, subscriptions, the inbox and watchers"),
        (name = "content", description = "Rich text: previews of Markdown"),
    )
)]
pub struct ApiDoc;

/// The two ways to call the API: the web app's session cookie, or a personal API token.
struct Credentials;

impl Modify for Credentials {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                SESSION_COOKIE,
                "Set by signing in (`POST /session`); used by the web app",
            ))),
        );
        let mut bearer = Http::new(HttpAuthScheme::Bearer);
        bearer.description = Some(
            "A personal API token (`Authorization: Bearer tfp_…`), limited to its scopes"
                .to_owned(),
        );
        components.add_security_scheme("token", SecurityScheme::Http(bearer));
    }
}
