//! Password hashing with argon2id (architecture.md §8). Hashing is slow and memory-hungry on
//! purpose (19 MiB, two passes), so it runs on the blocking pool and only a few hashes run at
//! once: a burst of sign-ins waits instead of exhausting memory.

use std::sync::{Arc, OnceLock};

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params};
use tokio::sync::Semaphore;

use crate::{Error, Result};

/// Hashes and checks passwords. Cheap to clone; clones share the limit.
#[derive(Clone)]
pub struct Passwords {
    permits: Arc<Semaphore>,
}

/// The result of checking a password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verified {
    /// The password matches; `rehash` is set when the stored hash uses older parameters.
    Match {
        rehash: Option<String>,
    },
    Mismatch,
}

impl Passwords {
    /// At most `parallel` hashes at a time (at least one).
    pub fn new(parallel: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(parallel.max(1))),
        }
    }

    /// One hash per CPU core.
    pub fn per_core() -> Self {
        Self::new(std::thread::available_parallelism().map_or(2, usize::from))
    }

    /// A new salted hash in the PHC string format.
    pub async fn hash(&self, password: &str) -> Result<String> {
        let password = password.to_owned();
        self.blocking(move || hash_now(&password)).await?
    }

    /// Checks `password` against `stored`. Without a stored hash (no such user, no password) the
    /// same work is done against a dummy hash, so the answer takes as long either way and does
    /// not tell whether the account exists.
    pub async fn verify(&self, password: &str, stored: Option<&str>) -> Result<Verified> {
        let password = password.to_owned();
        let stored = stored.map(str::to_owned);
        self.blocking(move || {
            let Some(stored) = stored else {
                let _ = check(&password, dummy_hash());
                return Ok(Verified::Mismatch);
            };
            if !check(&password, &stored) {
                return Ok(Verified::Mismatch);
            }
            let rehash = if outdated(&stored) {
                Some(hash_now(&password)?)
            } else {
                None
            };
            Ok(Verified::Match { rehash })
        })
        .await?
    }

    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T> {
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| Error::Internal("password hashing has stopped".to_owned()))?;
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|error| Error::Internal(format!("password hashing failed: {error}")))
    }
}

fn hash_now(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| Error::Internal(format!("cannot hash a password: {error}")))
}

fn check(password: &str, stored: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), stored)
        .is_ok()
}

/// Whether a stored hash was made with other parameters than today's.
fn outdated(stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return true;
    };
    let current = Params::default();
    let params = Params::try_from(&parsed);
    parsed.algorithm != Algorithm::Argon2id.ident()
        || params.map_or(true, |params| {
            (params.m_cost(), params.t_cost(), params.p_cost())
                != (current.m_cost(), current.t_cost(), current.p_cost())
        })
}

/// A hash of a random string, made once, for checks without a stored hash.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        hash_now("prostor: a password nobody has").unwrap_or_else(|_| {
            // Unreachable in practice; a fixed valid hash keeps the timing close.
            "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$\
             0KkBBQkvYYmdsj8tUqDRSWVwXamHGYOUANkZ6U7FlXc"
                .to_owned()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hashes_are_salted_and_verify() {
        let passwords = Passwords::new(2);
        let first = passwords.hash("correct horse").await.unwrap();
        let second = passwords.hash("correct horse").await.unwrap();
        assert_ne!(first, second, "every hash has its own salt");
        assert!(
            first.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "{first}"
        );

        assert_eq!(
            passwords
                .verify("correct horse", Some(&first))
                .await
                .unwrap(),
            Verified::Match { rehash: None }
        );
        assert_eq!(
            passwords.verify("wrong horse", Some(&first)).await.unwrap(),
            Verified::Mismatch
        );
        assert_eq!(
            passwords.verify("anything", None).await.unwrap(),
            Verified::Mismatch
        );
    }

    #[tokio::test]
    async fn hashes_with_old_parameters_are_upgraded() {
        let weak = Argon2::new(
            Algorithm::Argon2id,
            argon2::Version::V0x13,
            Params::new(8 * 1024, 1, 1, None).unwrap(),
        )
        .hash_password(b"correct horse")
        .unwrap()
        .to_string();
        let verified = Passwords::new(1)
            .verify("correct horse", Some(&weak))
            .await
            .unwrap();
        let Verified::Match {
            rehash: Some(rehash),
        } = verified
        else {
            panic!("expected a rehash, got {verified:?}");
        };
        assert!(rehash.contains("m=19456,t=2,p=1"), "{rehash}");
        assert!(!outdated(&rehash));
    }
}
