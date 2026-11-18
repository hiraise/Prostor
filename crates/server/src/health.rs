//! Liveness and readiness endpoints for orchestrators and load balancers (ADM-033).
//! No authentication and nothing sensitive in the answers.

use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use taskmaster_api::health::{LiveStatus, Liveness, Readiness, ReadyStatus};

use crate::AppState;
use crate::error::json;
use crate::lifecycle::Phase;

/// A ready node must answer within the usual probe timeout (1 s); a slower database counts as
/// unavailable.
const DATABASE_TIMEOUT: Duration = Duration::from_millis(800);

/// The process is up. Fails only when the process cannot answer at all.
#[utoipa::path(
    get,
    path = "/health/live",
    operation_id = "get_liveness",
    tag = "health",
    responses((status = OK, description = "The process is up", body = Liveness)),
)]
pub async fn live() -> Response {
    let body = Liveness {
        status: LiveStatus::Live,
        version: crate::VERSION.to_owned(),
    };
    no_store(json(StatusCode::OK, body))
}

/// The node can serve requests: startup is complete, migrations are applied and the database
/// answers.
#[utoipa::path(
    get,
    path = "/health/ready",
    operation_id = "get_readiness",
    tag = "health",
    responses(
        (status = OK, description = "Ready to serve requests", body = Readiness),
        (status = SERVICE_UNAVAILABLE, description = "Not ready; `status` tells why", body = Readiness),
    ),
)]
pub async fn ready(State(state): State<AppState>) -> Response {
    let status = match state.lifecycle.phase() {
        Phase::Starting => ReadyStatus::Starting,
        Phase::Migrating => ReadyStatus::Migrating,
        Phase::Stopping => ReadyStatus::Stopping,
        Phase::Ready if taskmaster_store::ping(&state.db, DATABASE_TIMEOUT).await => {
            ReadyStatus::Ready
        }
        Phase::Ready => ReadyStatus::DatabaseUnavailable,
    };
    let code = if status == ReadyStatus::Ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = Readiness {
        status,
        version: crate::VERSION.to_owned(),
    };
    no_store(json(code, body))
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
