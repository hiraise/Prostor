//! Accounts: the rules for usernames, emails, names and passwords (ACC, AUTH).

use std::fmt;

use crate::validation::{Problem, Rejection};

/// Longest username.
pub const USERNAME_MAX: usize = 50;
/// Longest full name.
pub const FULL_NAME_MAX: usize = 100;
/// Longest email address (RFC 5321 path limit).
pub const EMAIL_MAX: usize = 254;
/// Shortest password. The strength policy (AUTH-012) may ask for more.
pub const PASSWORD_MIN: usize = 8;
/// Longest password: long enough for any passphrase, short enough to bound hashing work.
pub const PASSWORD_MAX: usize = 256;

/// Usernames nobody may take: the guest account's, words with a meaning in addresses
/// (`/users/me`), and the query keywords of [`taskmaster_query::RESERVED_USERNAMES`] (ACC-017).
const RESERVED: &[&str] = &["guest", "me", "system"];

/// A sign-in name. Unique ignoring case. ASCII only (D-023): letters, digits, `.`, `_` and `-`,
/// starting with a letter or digit, so it never looks like an email and compares the same way in
/// every database locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Username(String);

impl Username {
    pub fn parse(raw: &str) -> Result<Self, Rejection> {
        let name = raw.trim();
        if name.is_empty() {
            return Err(Rejection::new(Problem::Required, "username is required"));
        }
        if name.chars().count() > USERNAME_MAX {
            return Err(Rejection::new(
                Problem::TooLong,
                format!("username is longer than {USERNAME_MAX} characters"),
            ));
        }
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
        let starts_well = name.starts_with(|c: char| c.is_ascii_alphanumeric());
        if !name.chars().all(allowed) || !starts_well {
            return Err(Rejection::new(
                Problem::Invalid,
                "username may contain Latin letters, digits, dots, underscores and hyphens, \
                 and must start with a letter or digit",
            ));
        }
        let lower = name.to_ascii_lowercase();
        if RESERVED.contains(&lower.as_str())
            || taskmaster_query::RESERVED_USERNAMES.contains(&lower.as_str())
        {
            return Err(Rejection::new(Problem::Taken, "this username is reserved"));
        }
        Ok(Self(name.to_owned()))
    }

    /// The guest account's name, which [`Username::parse`] refuses to everyone else.
    pub fn guest() -> Self {
        Self("guest".to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An email address, kept as typed (trimmed); unique ignoring case. Checked only for shape:
/// whether it receives mail is what verification (AUTH-014) finds out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn parse(raw: &str) -> Result<Self, Rejection> {
        let email = raw.trim();
        if email.is_empty() {
            return Err(Rejection::new(Problem::Required, "email is required"));
        }
        if email.chars().count() > EMAIL_MAX {
            return Err(Rejection::new(
                Problem::TooLong,
                format!("email is longer than {EMAIL_MAX} characters"),
            ));
        }
        let invalid = || Rejection::new(Problem::Invalid, "not an email address");
        let (local, domain) = email.split_once('@').ok_or_else(invalid)?;
        let forbidden =
            |c: char| c.is_whitespace() || c.is_control() || "@,;:<>()[]\\\"".contains(c);
        let domain_ok = domain.contains('.') && domain.split('.').all(|label| !label.is_empty());
        if local.is_empty()
            || local.chars().count() > 64
            || !domain_ok
            || local.chars().any(forbidden)
            || domain.chars().any(forbidden)
        {
            return Err(invalid());
        }
        Ok(Self(email.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The name shown wherever the person's activity appears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullName(String);

impl FullName {
    pub fn parse(raw: &str) -> Result<Self, Rejection> {
        plain_text(raw, FULL_NAME_MAX, "full name").map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Longest reason given for disabling an account.
pub const DISABLE_REASON_MAX: usize = 500;

/// How many background colors generated avatars have; each account gets one when it is created
/// and keeps it (ACC-017). The palette itself belongs to the web app's design tokens.
pub const AVATAR_COLORS: i16 = 8;

/// The word shown next to a disabled account (ACC-005). Purely descriptive: every label has the
/// same effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisabledLabel {
    Disabled,
    Suspended,
    /// Left the organization.
    Left,
    Blocked,
}

impl DisabledLabel {
    pub const ALL: [DisabledLabel; 4] = [
        DisabledLabel::Disabled,
        DisabledLabel::Suspended,
        DisabledLabel::Left,
        DisabledLabel::Blocked,
    ];

    pub fn key(self) -> &'static str {
        match self {
            DisabledLabel::Disabled => "disabled",
            DisabledLabel::Suspended => "suspended",
            DisabledLabel::Left => "left",
            DisabledLabel::Blocked => "blocked",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|label| label.key() == key)
    }
}

/// Validates the reason for disabling an account; empty is fine.
pub fn disable_reason(raw: &str) -> Result<String, Rejection> {
    let reason = raw.trim();
    if reason.chars().count() > DISABLE_REASON_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("reason is longer than {DISABLE_REASON_MAX} characters"),
        ));
    }
    Ok(reason.to_owned())
}

/// The username an invited person starts with (ACC-004): the local part of their email reduced to
/// the characters usernames allow, or `user` when nothing is left. Callers add a number when it
/// is taken ([`numbered_username`]).
pub fn username_from_email(email: &Email) -> String {
    let local = email.as_str().split('@').next().unwrap_or_default();
    // `anna+tracker@…` is Anna's address.
    let local = local.split('+').next().unwrap_or_default();
    let kept: String = local
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    let trimmed = kept.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    // Room for a number after it.
    let base: String = trimmed.chars().take(USERNAME_MAX - 6).collect();
    let base = base.to_ascii_lowercase();
    match Username::parse(&base) {
        Ok(name) => name.as_str().to_owned(),
        Err(_) => "user".to_owned(),
    }
}

/// `base` with a number from 2 on, for the `n`-th attempt (`n` = 0 is `base` itself).
pub fn numbered_username(base: &str, n: usize) -> String {
    if n == 0 {
        base.to_owned()
    } else {
        format!("{base}{}", n + 1)
    }
}

/// The full name an invited person starts with: the local part of their email (ACC-004).
pub fn full_name_from_email(email: &Email) -> String {
    let local = email.as_str().split('@').next().unwrap_or_default();
    let name: String = local.chars().take(FULL_NAME_MAX).collect();
    FullName::parse(&name)
        .map(|name| name.as_str().to_owned())
        .unwrap_or_else(|_| "User".to_owned())
}

/// Splits what someone typed into a list of addresses: separated by spaces, commas or semicolons
/// (ACC-004). Order is kept, repeats are dropped.
pub fn split_emails(raw: &str) -> Vec<String> {
    let mut seen = Vec::<String>::new();
    for part in raw.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let part = part.trim();
        if !part.is_empty() && !seen.iter().any(|s| s.eq_ignore_ascii_case(part)) {
            seen.push(part.to_owned());
        }
    }
    seen
}

/// Checks a password someone chooses. Spaces count: passphrases are welcome.
pub fn check_new_password(password: &str) -> Result<(), Rejection> {
    let length = password.chars().count();
    if length == 0 {
        return Err(Rejection::new(Problem::Required, "password is required"));
    }
    if length < PASSWORD_MIN {
        return Err(Rejection::new(
            Problem::TooShort,
            format!("password is shorter than {PASSWORD_MIN} characters"),
        ));
    }
    if length > PASSWORD_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("password is longer than {PASSWORD_MAX} characters"),
        ));
    }
    if password.trim().is_empty() {
        return Err(Rejection::new(
            Problem::Invalid,
            "password cannot be only spaces",
        ));
    }
    Ok(())
}

/// A single line of text: trimmed, not empty, at most `max` characters, no control characters.
pub fn plain_text(raw: &str, max: usize, what: &str) -> Result<String, Rejection> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(Rejection::new(
            Problem::Required,
            format!("{what} is required"),
        ));
    }
    if text.chars().count() > max {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("{what} is longer than {max} characters"),
        ));
    }
    if text.chars().any(char::is_control) {
        return Err(Rejection::new(
            Problem::Invalid,
            format!("{what} cannot contain line breaks or control characters"),
        ));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem<T: fmt::Debug>(result: Result<T, Rejection>) -> Problem {
        result.unwrap_err().problem
    }

    #[test]
    fn usernames() {
        for good in ["admin", "j.doe", "anna_k-2", "007", " padded "] {
            assert!(Username::parse(good).is_ok(), "{good}");
        }
        assert_eq!(Username::parse(" padded ").unwrap().as_str(), "padded");
        assert_eq!(problem(Username::parse("  ")), Problem::Required);
        assert_eq!(problem(Username::parse(&"a".repeat(51))), Problem::TooLong);
        for bad in ["-lead", ".lead", "a b", "иван", "a@b.c", "semi;colon"] {
            assert_eq!(problem(Username::parse(bad)), Problem::Invalid, "{bad}");
        }
        for reserved in ["guest", "Me", "SYSTEM", "none", "Open", "OR", "starred"] {
            assert_eq!(
                problem(Username::parse(reserved)),
                Problem::Taken,
                "{reserved}"
            );
        }
    }

    #[test]
    fn emails() {
        for good in [
            "a@b.co",
            "first.last+tag@mail.example.org",
            "иван@пример.рф",
        ] {
            assert!(Email::parse(good).is_ok(), "{good}");
        }
        assert_eq!(problem(Email::parse("")), Problem::Required);
        for bad in [
            "plain", "@b.co", "a@", "a@b", "a@b..co", "a b@c.co", "a@b@c.co", "<a@b.co>",
        ] {
            assert_eq!(problem(Email::parse(bad)), Problem::Invalid, "{bad}");
        }
        let long = format!("{}@example.com", "a".repeat(250));
        assert_eq!(problem(Email::parse(&long)), Problem::TooLong);
    }

    #[test]
    fn full_names() {
        assert_eq!(
            FullName::parse("  Анна Каренина ").unwrap().as_str(),
            "Анна Каренина"
        );
        assert_eq!(problem(FullName::parse(" ")), Problem::Required);
        assert_eq!(problem(FullName::parse("two\nlines")), Problem::Invalid);
        assert_eq!(problem(FullName::parse(&"x".repeat(101))), Problem::TooLong);
    }

    #[test]
    fn acc_004_invited_names_come_from_the_email() {
        let email = |raw: &str| Email::parse(raw).unwrap();
        assert_eq!(
            username_from_email(&email("Anna.K+work@example.com")),
            "anna.k"
        );
        assert_eq!(username_from_email(&email("__bob@example.com")), "bob");
        assert_eq!(username_from_email(&email("иван@пример.рф")), "user");
        assert_eq!(username_from_email(&email("me@example.com")), "user");
        assert_eq!(
            username_from_email(&email(&format!("{}@example.com", "a".repeat(60)))).len(),
            USERNAME_MAX - 6
        );
        assert_eq!(numbered_username("anna", 0), "anna");
        assert_eq!(numbered_username("anna", 1), "anna2");
        assert_eq!(full_name_from_email(&email("Anna.K@example.com")), "Anna.K");
        assert_eq!(
            split_emails("a@x.co, b@x.co;c@x.co\n A@X.CO  "),
            ["a@x.co", "b@x.co", "c@x.co"]
        );
    }

    #[test]
    fn acc_005_disabled_labels() {
        for label in DisabledLabel::ALL {
            assert_eq!(DisabledLabel::parse(label.key()), Some(label));
        }
        assert_eq!(disable_reason("  left in May ").unwrap(), "left in May");
        assert_eq!(
            disable_reason(&"x".repeat(501)).unwrap_err().problem,
            Problem::TooLong
        );
    }

    #[test]
    fn passwords() {
        assert!(check_new_password("correct horse").is_ok());
        assert!(check_new_password("пароль12").is_ok());
        assert_eq!(problem(check_new_password("")), Problem::Required);
        assert_eq!(problem(check_new_password("short")), Problem::TooShort);
        assert_eq!(
            problem(check_new_password(&"x".repeat(257))),
            Problem::TooLong
        );
        assert_eq!(problem(check_new_password("          ")), Problem::Invalid);
    }
}
