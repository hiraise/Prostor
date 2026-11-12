//! Who is calling and how long a sign-in lasts: sessions, personal API tokens and their scopes
//! (AUTH-003, AUTH-004, AUTH-029, AUTH-030). Design: decisions.md D-023.

use time::{Duration, OffsetDateTime};

use crate::validation::{Problem, Rejection};
use crate::{Actor, UserId};

/// What a personal API token may be used for (AUTH-030). A token never has more rights than its
/// owner; a scope only narrows them. Changing the token owner's credentials (password, tokens,
/// sessions) needs a signed-in session and is never possible with a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// Read projects, issues, boards, reports and everything else of the tracker.
    TrackerRead,
    /// Create and change them; includes reading.
    TrackerWrite,
    /// Read users, groups, roles and settings.
    AdminRead,
    /// Change them; includes reading.
    AdminWrite,
}

impl Scope {
    pub const ALL: [Scope; 4] = [
        Scope::TrackerRead,
        Scope::TrackerWrite,
        Scope::AdminRead,
        Scope::AdminWrite,
    ];

    /// Stable key, stored with tokens and used in the API.
    pub fn key(self) -> &'static str {
        match self {
            Scope::TrackerRead => "tracker.read",
            Scope::TrackerWrite => "tracker.write",
            Scope::AdminRead => "admin.read",
            Scope::AdminWrite => "admin.write",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.key() == key)
    }

    /// The scope this one includes.
    fn includes(self) -> Option<Scope> {
        match self {
            Scope::TrackerWrite => Some(Scope::TrackerRead),
            Scope::AdminWrite => Some(Scope::AdminRead),
            Scope::TrackerRead | Scope::AdminRead => None,
        }
    }
}

/// The scopes of one token, kept minimal: a write scope stands for its read scope too.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scopes(Vec<Scope>);

impl Scopes {
    /// Validates the keys someone chose: at least one, all known.
    pub fn parse(keys: &[String]) -> Result<Self, Rejection> {
        let mut scopes = Vec::with_capacity(keys.len());
        for key in keys {
            let scope = Scope::parse(key).ok_or_else(|| {
                Rejection::new(Problem::Invalid, format!("unknown scope {key:?}"))
            })?;
            scopes.push(scope);
        }
        if scopes.is_empty() {
            return Err(Rejection::new(
                Problem::Required,
                "choose at least one scope",
            ));
        }
        Ok(Self::normalized(scopes))
    }

    /// Scopes read back from storage; unknown keys (from a newer version) are ignored.
    pub fn from_stored(keys: &[String]) -> Self {
        Self::normalized(keys.iter().filter_map(|key| Scope::parse(key)).collect())
    }

    fn normalized(mut scopes: Vec<Scope>) -> Self {
        let included: Vec<Scope> = scopes.iter().filter_map(|scope| scope.includes()).collect();
        scopes.retain(|scope| !included.contains(scope));
        scopes.sort();
        scopes.dedup();
        Self(scopes)
    }

    pub fn allows(&self, scope: Scope) -> bool {
        self.0
            .iter()
            .any(|held| *held == scope || held.includes() == Some(scope))
    }

    pub fn iter(&self) -> impl Iterator<Item = Scope> + '_ {
        self.0.iter().copied()
    }

    pub fn keys(&self) -> Vec<String> {
        self.iter().map(|scope| scope.key().to_owned()).collect()
    }
}

/// Longest token name.
pub const TOKEN_NAME_MAX: usize = 100;

/// How long sessions last, from the settings `session_idle_minutes` and `remember_me_days`
/// (AUTH-004). Read on every check, so a changed setting applies to existing sessions at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionPolicy {
    /// A session without "remember me" ends after this long without requests.
    pub idle: Duration,
    /// A session with "remember me" ends this long after sign-in, used or not.
    pub remember: Duration,
}

/// How often a session's last activity is written: often enough for the idle timeout (at least
/// ten minutes), rarely enough that requests do not turn into writes.
pub const TOUCH_INTERVAL: Duration = Duration::minutes(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCheck {
    /// The session is valid; `touch` asks to record the activity.
    Active {
        touch: bool,
    },
    Expired,
}

/// The state of a session at `now`.
pub fn check_session(
    policy: SessionPolicy,
    remember: bool,
    created_at: OffsetDateTime,
    last_seen_at: OffsetDateTime,
    now: OffsetDateTime,
) -> SessionCheck {
    let ends = if remember {
        created_at + policy.remember
    } else {
        last_seen_at + policy.idle
    };
    if now >= ends {
        SessionCheck::Expired
    } else {
        SessionCheck::Active {
            touch: now - last_seen_at >= TOUCH_INTERVAL,
        }
    }
}

/// How long the browser keeps the session cookie: until the browser closes without "remember
/// me" (`None`), otherwise as long as the session may last.
pub fn cookie_lifetime(policy: SessionPolicy, remember: bool) -> Option<Duration> {
    remember.then_some(policy.remember)
}

/// How a request proved who sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// A browser session after password sign-in. While `must_change_password` is set, the
    /// session may only change the password, read itself and sign out (AUTH-010).
    Session { id: i64, must_change_password: bool },
    /// A personal API token, limited to its scopes.
    Token { id: i64, scopes: Scopes },
}

/// The signed-in user behind a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub user: UserId,
    pub credential: Credential,
}

impl Principal {
    /// Whether the credential may be used for `scope`; sessions may be used for everything.
    pub fn allows(&self, scope: Scope) -> bool {
        match &self.credential {
            Credential::Session { .. } => true,
            Credential::Token { scopes, .. } => scopes.allows(scope),
        }
    }

    /// The session, when the request came from a signed-in browser.
    pub fn session_id(&self) -> Option<i64> {
        match self.credential {
            Credential::Session { id, .. } => Some(id),
            Credential::Token { .. } => None,
        }
    }

    pub fn actor(&self) -> Actor {
        Actor::User(self.user)
    }

    /// The person must choose a new password before anything else (AUTH-010).
    pub fn must_change_password(&self) -> bool {
        matches!(
            self.credential,
            Credential::Session {
                must_change_password: true,
                ..
            }
        )
    }
}

/// Why a one-time link was emailed (AUTH-008, AUTH-014, ACC-004). The link carries a random
/// secret; only its hash is stored, and using the link deletes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkPurpose {
    /// Choose a new password after forgetting it.
    PasswordReset,
    /// Confirm that an address receives mail.
    EmailVerification,
    /// Join trackflow: choose a password for an account an administrator created.
    Invitation,
}

impl LinkPurpose {
    pub const ALL: [LinkPurpose; 3] = [
        LinkPurpose::PasswordReset,
        LinkPurpose::EmailVerification,
        LinkPurpose::Invitation,
    ];

    pub fn key(self) -> &'static str {
        match self {
            LinkPurpose::PasswordReset => "password_reset",
            LinkPurpose::EmailVerification => "email_verification",
            LinkPurpose::Invitation => "invitation",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|purpose| purpose.key() == key)
    }

    /// How long a link works: short for a password reset, which takes over an account; long for
    /// an invitation, which people open when they get to it.
    pub fn lifetime(self) -> Duration {
        match self {
            LinkPurpose::PasswordReset => Duration::hours(1),
            LinkPurpose::EmailVerification => Duration::days(3),
            LinkPurpose::Invitation => Duration::days(14),
        }
    }
}

/// A new password-reset link is not sent while the previous one is younger than this, so nobody
/// can flood a person's inbox (AUTH-008).
pub const RESET_RESEND_INTERVAL: Duration = Duration::minutes(1);

/// Codes people type or copy from a log: Crockford's base32 (no I, L, O, U), so nothing looks
/// alike, grouped by hyphens. Used for the setup code and generated passwords.
pub mod codes {
    /// The 32 symbols; a random byte's low five bits pick one without bias.
    pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    /// Turns random bytes into `groups` groups of `size` symbols: `K7QM-4XPD-9TRB`. Needs
    /// `groups * size` bytes; extra bytes are ignored.
    pub fn format(random: &[u8], groups: usize, size: usize) -> String {
        let mut code = String::with_capacity(groups * (size + 1));
        for (index, byte) in random.iter().take(groups * size).enumerate() {
            if index > 0 && index % size == 0 {
                code.push('-');
            }
            code.push(char::from(ALPHABET[usize::from(byte & 31)]));
        }
        code
    }

    /// The canonical form of a typed code: case, spaces and hyphens do not matter, and letters
    /// that look like digits count as those digits.
    pub fn normalize(typed: &str) -> String {
        typed
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            })
            .collect()
    }

    /// Compares a typed code with the real one in constant time.
    pub fn matches(typed: &str, actual: &str) -> bool {
        let typed = normalize(typed);
        let actual = normalize(actual);
        if typed.len() != actual.len() {
            return false;
        }
        typed
            .bytes()
            .zip(actual.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_owned()).collect()
    }

    #[test]
    fn auth_030_write_scopes_include_read_scopes() {
        let scopes =
            Scopes::parse(&keys(&["tracker.read", "tracker.write", "admin.read"])).unwrap();
        assert_eq!(scopes.keys(), ["tracker.write", "admin.read"]);
        assert!(scopes.allows(Scope::TrackerRead));
        assert!(scopes.allows(Scope::TrackerWrite));
        assert!(scopes.allows(Scope::AdminRead));
        assert!(!scopes.allows(Scope::AdminWrite));

        let read_only = Scopes::parse(&keys(&["tracker.read"])).unwrap();
        assert!(read_only.allows(Scope::TrackerRead));
        assert!(!read_only.allows(Scope::TrackerWrite));
        assert!(!read_only.allows(Scope::AdminRead));
    }

    #[test]
    fn auth_030_scopes_are_validated() {
        assert_eq!(Scopes::parse(&[]).unwrap_err().problem, Problem::Required);
        assert_eq!(
            Scopes::parse(&keys(&["tracker.read", "everything"]))
                .unwrap_err()
                .problem,
            Problem::Invalid
        );
        assert_eq!(
            Scopes::from_stored(&keys(&["future.scope", "admin.write"])).keys(),
            ["admin.write"]
        );
    }

    #[test]
    fn sessions_count_every_scope_tokens_only_theirs() {
        let session = Principal {
            user: 1,
            credential: Credential::Session {
                id: 5,
                must_change_password: false,
            },
        };
        let token = Principal {
            user: 1,
            credential: Credential::Token {
                id: 7,
                scopes: Scopes::parse(&keys(&["tracker.read"])).unwrap(),
            },
        };
        assert!(Scope::ALL.iter().all(|scope| session.allows(*scope)));
        assert!(token.allows(Scope::TrackerRead) && !token.allows(Scope::AdminRead));
        assert_eq!(session.session_id(), Some(5));
        assert_eq!(token.session_id(), None);
        assert_eq!(token.actor(), Actor::User(1));
        assert!(!session.must_change_password() && !token.must_change_password());
    }

    #[test]
    fn link_purposes() {
        for purpose in LinkPurpose::ALL {
            assert_eq!(LinkPurpose::parse(purpose.key()), Some(purpose));
        }
        assert!(LinkPurpose::PasswordReset.lifetime() < LinkPurpose::Invitation.lifetime());
    }

    const POLICY: SessionPolicy = SessionPolicy {
        idle: Duration::hours(1),
        remember: Duration::days(30),
    };

    #[test]
    fn auth_004_a_session_without_remember_me_ends_when_idle() {
        let signed_in = datetime!(2026-10-01 09:00 UTC);
        let seen = datetime!(2026-10-01 12:00 UTC);
        let check = |now| check_session(POLICY, false, signed_in, seen, now);
        assert_eq!(
            check(datetime!(2026-10-01 12:00:30 UTC)),
            SessionCheck::Active { touch: false }
        );
        assert_eq!(
            check(datetime!(2026-10-01 12:59 UTC)),
            SessionCheck::Active { touch: true }
        );
        assert_eq!(
            check(datetime!(2026-10-01 13:00 UTC)),
            SessionCheck::Expired
        );
    }

    #[test]
    fn auth_003_a_remembered_session_lasts_its_duration_from_sign_in() {
        let signed_in = datetime!(2026-10-01 09:00 UTC);
        let check = |seen, now| check_session(POLICY, true, signed_in, seen, now);
        // Idle for days, still valid.
        assert!(matches!(
            check(signed_in, datetime!(2026-10-20 09:00 UTC)),
            SessionCheck::Active { .. }
        ));
        // Used a minute ago, but the 30 days are over.
        assert_eq!(
            check(
                datetime!(2026-10-31 08:59 UTC),
                datetime!(2026-10-31 09:00 UTC)
            ),
            SessionCheck::Expired
        );
        assert_eq!(cookie_lifetime(POLICY, true), Some(Duration::days(30)));
        assert_eq!(cookie_lifetime(POLICY, false), None);
    }

    #[test]
    fn codes_are_unambiguous_and_forgiving() {
        let code = codes::format(&[0, 1, 31, 32, 255, 18, 7, 9, 99], 3, 3);
        assert_eq!(code, "01Z-0ZJ-793");
        assert_eq!(codes::normalize(" o1z-OzJ 793 "), "01Z0ZJ793");
        assert_eq!(codes::normalize("il"), "11");
        assert!(codes::matches("01z0zj793", &code));
        assert!(codes::matches("OIZ-OZJ-793", &code));
        assert!(!codes::matches("01Z-0ZJ-794", &code));
        assert!(!codes::matches("01Z-0ZJ", &code));
    }
}
