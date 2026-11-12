//! Guessing protection for sign-in and setup (architecture.md §8; D-013, D-023). Failures are
//! counted per login and address, and per address: a guesser runs out of attempts, while someone
//! guessing a colleague's login from elsewhere cannot lock the colleague out. Counters live in
//! the node's memory; the configurable, shared version is AUTH-017.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// What failures are counted against.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// A login (lowercased) from one address.
    Login(String, Option<IpAddr>),
    /// Every sign-in from one address.
    Address(Option<IpAddr>),
    /// Setup codes from one address.
    Setup(Option<IpAddr>),
}

impl Key {
    pub fn login(login: &str, ip: Option<IpAddr>) -> Self {
        Key::Login(login.trim().to_lowercase(), ip)
    }

    fn limit(&self, limits: &Limits) -> u32 {
        match self {
            Key::Login(..) => limits.per_login,
            Key::Address(_) => limits.per_address,
            Key::Setup(_) => limits.setup,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Failures for one login from one address.
    pub per_login: u32,
    /// Failures from one address for any logins (a whole office may share one).
    pub per_address: u32,
    /// Wrong setup codes from one address.
    pub setup: u32,
    /// Failures count for this long after the first one.
    pub window: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            per_login: 10,
            per_address: 50,
            setup: 10,
            window: Duration::from_secs(15 * 60),
        }
    }
}

/// Counters are dropped when there are more than this many, oldest windows first; it bounds
/// memory under a flood of made-up logins.
const MAX_KEYS: usize = 100_000;

#[derive(Debug, Clone, Copy)]
struct Window {
    started: Instant,
    failures: u32,
}

#[derive(Debug, Default)]
pub struct Throttle {
    limits: Limits,
    windows: Mutex<HashMap<Key, Window>>,
}

impl Throttle {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            windows: Mutex::default(),
        }
    }

    /// `Err(retry_after)` when any of the keys has used up its attempts.
    pub fn check(&self, keys: &[Key], now: Instant) -> Result<(), Duration> {
        let windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        let wait = keys
            .iter()
            .filter_map(|key| {
                let window = windows.get(key)?;
                let ends = window.started + self.limits.window;
                (now < ends && window.failures >= key.limit(&self.limits))
                    .then(|| ends.saturating_duration_since(now))
            })
            .max();
        match wait {
            Some(wait) => Err(wait.max(Duration::from_secs(1))),
            None => Ok(()),
        }
    }

    /// Counts a failure against every key.
    pub fn failed(&self, keys: &[Key], now: Instant) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        if windows.len() >= MAX_KEYS {
            let window = self.limits.window;
            windows.retain(|_, counted| now.saturating_duration_since(counted.started) < window);
            if windows.len() >= MAX_KEYS {
                windows.clear();
            }
        }
        for key in keys {
            let counted = windows.entry(key.clone()).or_insert(Window {
                started: now,
                failures: 0,
            });
            if now.saturating_duration_since(counted.started) >= self.limits.window {
                *counted = Window {
                    started: now,
                    failures: 0,
                };
            }
            counted.failures = counted.failures.saturating_add(1);
        }
    }

    /// Forgets the failures of a key after a success.
    pub fn succeeded(&self, key: &Key) {
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: Limits = Limits {
        per_login: 3,
        per_address: 5,
        setup: 2,
        window: Duration::from_secs(60),
    };

    fn ip(last: u8) -> Option<IpAddr> {
        Some(IpAddr::from([192, 0, 2, last]))
    }

    #[test]
    fn a_login_is_blocked_after_its_failures_until_the_window_ends() {
        let throttle = Throttle::new(LIMITS);
        let start = Instant::now();
        let keys = [Key::login("Anna", ip(1)), Key::Address(ip(1))];
        for _ in 0..3 {
            assert!(throttle.check(&keys, start).is_ok());
            throttle.failed(&keys, start);
        }
        let wait = throttle
            .check(&keys, start + Duration::from_secs(10))
            .unwrap_err();
        assert_eq!(wait, Duration::from_secs(50));
        // The same login from another address is not blocked.
        assert!(
            throttle
                .check(&[Key::login("anna", ip(2)), Key::Address(ip(2))], start)
                .is_ok()
        );
        assert!(
            throttle
                .check(&keys, start + Duration::from_secs(60))
                .is_ok()
        );
    }

    #[test]
    fn an_address_is_blocked_after_failures_on_many_logins() {
        let throttle = Throttle::new(LIMITS);
        let now = Instant::now();
        for user in ["a", "b", "c", "d", "e"] {
            throttle.failed(&[Key::login(user, ip(1)), Key::Address(ip(1))], now);
        }
        assert!(
            throttle
                .check(&[Key::login("f", ip(1)), Key::Address(ip(1))], now)
                .is_err()
        );
    }

    #[test]
    fn success_clears_the_login_but_not_the_address() {
        let throttle = Throttle::new(LIMITS);
        let now = Instant::now();
        let login = Key::login("anna", ip(1));
        throttle.failed(&[login.clone(), Key::Address(ip(1))], now);
        throttle.failed(&[login.clone(), Key::Address(ip(1))], now);
        throttle.succeeded(&login);
        throttle.failed(std::slice::from_ref(&login), now);
        throttle.failed(std::slice::from_ref(&login), now);
        assert!(throttle.check(&[login], now).is_ok());
    }

    #[test]
    fn setup_codes_have_their_own_limit() {
        let throttle = Throttle::new(LIMITS);
        let now = Instant::now();
        throttle.failed(&[Key::Setup(ip(1))], now);
        assert!(throttle.check(&[Key::Setup(ip(1))], now).is_ok());
        throttle.failed(&[Key::Setup(ip(1))], now);
        assert!(throttle.check(&[Key::Setup(ip(1))], now).is_err());
    }
}
