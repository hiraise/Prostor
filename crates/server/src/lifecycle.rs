//! Where the node is in its life: starting, migrating, ready, stopping (ADM-033).

use std::sync::atomic::{AtomicU8, Ordering};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the database.
    Starting,
    /// Applying migrations.
    Migrating,
    Ready,
    Stopping,
}

#[derive(Debug, Default)]
pub struct Lifecycle {
    phase: AtomicU8,
}

impl Lifecycle {
    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Acquire) {
            0 => Phase::Starting,
            1 => Phase::Migrating,
            2 => Phase::Ready,
            _ => Phase::Stopping,
        }
    }

    pub fn set(&self, phase: Phase) {
        let value = match phase {
            Phase::Starting => 0,
            Phase::Migrating => 1,
            Phase::Ready => 2,
            Phase::Stopping => 3,
        };
        self.phase.store(value, Ordering::Release);
    }
}

/// Guards endpoints that need the database: until the node is ready they answer `unavailable`.
pub async fn require_ready(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    match state.lifecycle.phase() {
        Phase::Ready => next.run(request).await,
        Phase::Stopping => ApiError::unavailable("The server is shutting down").into_response(),
        Phase::Starting | Phase::Migrating => {
            ApiError::unavailable("The server is starting").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_round_trip() {
        let lifecycle = Lifecycle::default();
        assert_eq!(lifecycle.phase(), Phase::Starting);
        for phase in [
            Phase::Migrating,
            Phase::Ready,
            Phase::Stopping,
            Phase::Starting,
        ] {
            lifecycle.set(phase);
            assert_eq!(lifecycle.phase(), phase);
        }
    }
}
