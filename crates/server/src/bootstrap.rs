//! The configuration every client loads before rendering (ADM-002).

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use taskmaster_api::bootstrap::{Bootstrap, DateFormat as Format, Limits, WorkSchedule};
use taskmaster_api::error::ErrorBody;
use taskmaster_api::preferences::HomePage;
use taskmaster_domain::settings::{BASE_URL, DEFAULT_DATE_FORMAT, DEFAULT_HOME, UPLOAD_MAX_BYTES};
use taskmaster_domain::settings::{DateFormat as DomainFormat, Home};

use crate::AppState;
use crate::error::{ApiError, json};

/// What the web app needs before it renders. Available without signing in.
#[utoipa::path(
    get,
    path = "/bootstrap",
    operation_id = "get_bootstrap",
    tag = "bootstrap",
    responses(
        (status = OK, description = "Current configuration", body = Bootstrap),
        (status = SERVICE_UNAVAILABLE, description = "The server is starting", body = ErrorBody),
    ),
)]
pub async fn get(State(state): State<AppState>) -> Result<Response, ApiError> {
    let settings = state.settings.get();
    let body = Bootstrap {
        version: crate::VERSION.to_owned(),
        base_url: settings.get(&BASE_URL),
        setup_required: crate::setup::required(&state).await?,
        // The instance email switch and a transport (NTF-002): the same state every warning reads.
        email_enabled: state.settings.mail().enabled,
        gravatar: state.gravatar.is_some(),
        // Public, not sensitive: the fallbacks of a person without preferences of their own.
        default_home: match settings.get(&DEFAULT_HOME) {
            Home::Issues => HomePage::Issues,
            Home::Projects => HomePage::Projects,
            Home::Boards => HomePage::Boards,
        },
        default_date_format: match settings.get(&DEFAULT_DATE_FORMAT) {
            DomainFormat::Locale => Format::Locale,
            DomainFormat::DmyDot => Format::DmyDot,
            DomainFormat::DmyName => Format::DmyName,
            DomainFormat::Iso => Format::Iso,
        },
        limits: Limits {
            upload_max_bytes: settings.get(&UPLOAD_MAX_BYTES).min(state.upload_ceiling),
        },
        work_schedule: {
            // Fixed until the schedule settings (TIME-001…TIME-003, P2).
            let schedule = taskmaster_domain::duration::Schedule::default();
            WorkSchedule {
                day_minutes: schedule.day,
                week_days: schedule.week_days,
            }
        },
    };
    let mut response = json(StatusCode::OK, body);
    // Settings change at any time; clients must not reuse a stale copy.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(response)
}
