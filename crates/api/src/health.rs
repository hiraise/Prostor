//! Liveness and readiness for orchestrators and load balancers (ADM-033).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The process is up and answering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Liveness {
    pub status: LiveStatus,
    /// Server version.
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveStatus {
    Live,
}

/// Whether the node can serve requests. Answered with HTTP 200 only when `status` is `ready`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Readiness {
    pub status: ReadyStatus,
    /// Server version.
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadyStatus {
    /// Serving requests.
    Ready,
    /// Waiting for the database or loading.
    Starting,
    /// Applying database migrations.
    Migrating,
    /// Shutting down.
    Stopping,
    /// Started, but the database does not answer now.
    DatabaseUnavailable,
}
