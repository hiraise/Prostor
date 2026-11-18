//! The node's cache of compiled access (architecture.md §4, ACC-062): one entry per person, built
//! on first use and dropped whenever any node reports an access change over the bus, or when this
//! node changes access itself. A load that started before a drop is not cached, so an entry never
//! outlives the change that made it stale.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use taskmaster_domain::UserId;
use taskmaster_domain::access::Access;
use taskmaster_domain::auth::Principal;
use taskmaster_store::PgPool;
use taskmaster_store::bus::{self, BusEvent};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;

/// Most people whose access one node keeps; beyond that the cache starts over.
const CAPACITY: usize = 20_000;

#[derive(Clone, Default)]
pub struct AccessCache {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Increases with every drop.
    generation: AtomicU64,
    entries: Mutex<HashMap<UserId, Arc<Access>>>,
}

impl AccessCache {
    /// The person's access, from the cache or the database.
    pub async fn get(&self, pool: &PgPool, user: UserId) -> Result<Arc<Access>, ApiError> {
        if let Some(found) = self.entries().get(&user) {
            return Ok(Arc::clone(found));
        }
        let generation = self.inner.generation.load(Ordering::Acquire);
        let access = Arc::new(taskmaster_service::access::load(pool, user).await?);
        let mut entries = self.entries();
        if self.inner.generation.load(Ordering::Acquire) == generation {
            if entries.len() >= CAPACITY {
                entries.clear();
            }
            entries.insert(user, Arc::clone(&access));
        }
        Ok(access)
    }

    /// Drops everything: access changed somewhere.
    pub fn clear(&self) {
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        self.entries().clear();
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<UserId, Arc<Access>>> {
        self.inner
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Drops the cache on every access notification and whenever notifications may have been
    /// missed.
    pub fn follow(
        &self,
        mut events: broadcast::Receiver<BusEvent>,
        stop: CancellationToken,
    ) -> JoinHandle<()> {
        let cache = self.clone();
        tokio::spawn(async move {
            loop {
                let event = tokio::select! {
                    () = stop.cancelled() => return,
                    event = events.recv() => event,
                };
                match event {
                    Ok(BusEvent::Notified { channel, .. }) if channel == bus::ACCESS => {
                        cache.clear()
                    }
                    Ok(BusEvent::Notified { .. }) => {}
                    Ok(BusEvent::Resync) | Err(broadcast::error::RecvError::Lagged(_)) => {
                        cache.clear();
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        })
    }
}

/// The signed-in caller with their access.
#[derive(Debug, Clone)]
pub struct Viewer {
    pub principal: Principal,
    pub access: Arc<Access>,
}

impl FromRequestParts<AppState> for Viewer {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let Caller(principal) = Caller::from_request_parts(parts, state).await?;
        let access = state.access.get(&state.db, principal.user).await?;
        Ok(Viewer { principal, access })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_moves_the_generation() {
        let cache = AccessCache::default();
        cache.entries().insert(1, Arc::new(Access::none(1)));
        let before = cache.inner.generation.load(Ordering::Acquire);
        cache.clear();
        assert!(cache.entries().is_empty());
        assert_eq!(cache.inner.generation.load(Ordering::Acquire), before + 1);
    }
}
