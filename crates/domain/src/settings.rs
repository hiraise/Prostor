//! Instance-wide settings: one typed registry with defaults and validation (ADM-002).
//!
//! Every setting is declared here once: key, value type, default and validation. The store keeps
//! only values that differ from the default, as JSON. A value always passes [`prepare`] before it
//! is stored, and a stored value that no longer validates (for example after a rule got stricter)
//! falls back to the default instead of stopping the server.

use std::collections::{HashMap, HashSet};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use time::Duration;

use crate::auth::SessionPolicy;

/// Declaration of one setting with value type `T`.
pub struct Setting<T> {
    /// Stable key; also the key of the UI label in i18n messages.
    pub key: &'static str,
    pub default: fn() -> T,
    /// Checks a new value and returns its normalized form, or an English reason.
    pub validate: fn(T) -> Result<T, String>,
    /// A secret value (NTF-004): sealed with the instance key in its own table, kept out of
    /// audit events and of every answer.
    pub secret: bool,
}

/// A setting with its value type erased, for the registry and the store.
pub trait Definition: Sync {
    fn key(&self) -> &'static str;
    fn default_json(&self) -> Value;
    /// Parses and validates a JSON value; returns the normalized JSON to store.
    fn parse(&self, value: &Value) -> Result<Value, SettingError>;
    /// Whether the value is a secret (NTF-004).
    fn secret(&self) -> bool;
}

impl<T: Serialize + DeserializeOwned> Definition for Setting<T> {
    fn key(&self) -> &'static str {
        self.key
    }

    fn secret(&self) -> bool {
        self.secret
    }

    fn default_json(&self) -> Value {
        to_json((self.default)())
    }

    fn parse(&self, value: &Value) -> Result<Value, SettingError> {
        let typed: T =
            serde_json::from_value(value.clone()).map_err(|error| SettingError::WrongType {
                key: self.key.to_owned(),
                message: error.to_string(),
            })?;
        let normalized = (self.validate)(typed).map_err(|message| SettingError::Invalid {
            key: self.key.to_owned(),
            message,
        })?;
        Ok(to_json(normalized))
    }
}

fn to_json<T: Serialize>(value: T) -> Value {
    // Setting types are plain data (numbers, strings, options, lists), which always serialize.
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingError {
    #[error("there is no setting {0:?}")]
    Unknown(String),
    #[error("setting {key} has a value of the wrong type: {message}")]
    WrongType { key: String, message: String },
    #[error("setting {key} has an invalid value: {message}")]
    Invalid { key: String, message: String },
}

// ---- The registry. Add a setting here and to `ALL`; its feature row names the key. ----

/// Canonical absolute URL of the instance, used in generated links (ADM-003). `None` until set.
pub static BASE_URL: Setting<Option<String>> = Setting {
    key: "base_url",
    default: || None,
    validate: validate_base_url,
    secret: false,
};

/// Largest size of one uploaded file in bytes (ADM-004). Default 10 MiB.
pub static UPLOAD_MAX_BYTES: Setting<u64> = Setting {
    key: "upload_max_bytes",
    default: || 10 * 1024 * 1024,
    validate: |bytes| {
        if bytes == 0 {
            Err("must be at least 1 byte".to_owned())
        } else {
            Ok(bytes)
        }
    },
    secret: false,
};

/// Language of the instance (ADM-017): query and command keywords, built-in names and
/// notifications. Chosen in the setup wizard (ADM-047); takes effect with ADM-017.
pub static SYSTEM_LANGUAGE: Setting<Language> = Setting {
    key: "system_language",
    default: || Language::En,
    validate: Ok,
    secret: false,
};

/// Time zone for people who have not chosen their own, and of server-side calendar logic
/// (ADM-015). An IANA name such as `Europe/Berlin`. Chosen in the setup wizard (ADM-047).
pub static DEFAULT_TIME_ZONE: Setting<String> = Setting {
    key: "default_time_zone",
    default: || "UTC".to_owned(),
    validate: validate_time_zone,
    secret: false,
};

/// Minutes without requests after which a session without "remember me" ends (AUTH-004).
pub static SESSION_IDLE_MINUTES: Setting<u32> = Setting {
    key: "session_idle_minutes",
    default: || 60,
    validate: |minutes| in_range(minutes, 10, 12 * 60),
    secret: false,
};

/// Days a session with "remember me" lasts after sign-in (AUTH-003, AUTH-004).
pub static REMEMBER_ME_DAYS: Setting<u32> = Setting {
    key: "remember_me_days",
    default: || 30,
    validate: |days| in_range(days, 1, 365),
    secret: false,
};

/// The outgoing mail server as an SMTP URL, credentials included (`smtp://user:pass@host:587`,
/// NTF-004). Empty: the transport falls back to `PROSTOR_SMTP_URL`, else email is off. Secret:
/// the URL usually names an account.
pub static SMTP_URL: Setting<String> = Setting {
    key: "smtp_url",
    default: || String::new(),
    validate: Ok,
    secret: true,
};

/// The SMTP user name, when the server wants one the URL does not carry (NTF-004). Secret.
pub static SMTP_USERNAME: Setting<String> = Setting {
    key: "smtp_username",
    default: || String::new(),
    validate: Ok,
    secret: true,
};

/// The SMTP password (a provider's app password when 2-step verification is on, NTF-004). Secret.
pub static SMTP_PASSWORD: Setting<String> = Setting {
    key: "smtp_password",
    default: || String::new(),
    validate: Ok,
    secret: true,
};

/// The sender of every email, an address with an optional name (`prostor <t@example.com>`,
/// NTF-006). Empty: the transport falls back to `PROSTOR_MAIL_FROM`.
pub static MAIL_FROM: Setting<String> = Setting {
    key: "mail_from",
    default: || String::new(),
    validate: validate_address,
    secret: false,
};

/// Where replies go (NTF-006); empty means replies go to the sender.
pub static MAIL_REPLY_TO: Setting<String> = Setting {
    key: "mail_reply_to",
    default: || String::new(),
    validate: validate_address,
    secret: false,
};

/// The instance email switch (NTF-002): off until an administrator has configured a transport,
/// tested it and switched email on. With no mail setting stored at all, the transport from the
/// environment decides (D-026).
pub static EMAIL_ENABLED: Setting<bool> = Setting {
    key: "email_enabled",
    default: || false,
    validate: Ok,
    secret: false,
};

/// The page a person lands on after sign-in when they have not chosen their own (UI-004): the
/// fallback of `preferences::home`, whose default agrees with this one.
pub static DEFAULT_HOME: Setting<Home> = Setting {
    key: "default_home",
    default: || Home::Issues,
    validate: Ok,
    secret: false,
};

/// Browser origins allowed to call `/api/v1` from other sites (INT-056): cross-origin access for
/// token-bearing scripts, one origin per line on the server tab. Empty (the default): same-origin
/// only. Credentials never ride cross-origin — sessions stay same-origin — so the CORS answers
/// never allow them.
pub static API_ALLOWED_ORIGINS: Setting<Vec<String>> = Setting {
    key: "api.allowed_origins",
    default: Vec::new,
    validate: validate_origins,
    secret: false,
};

/// How dates and times are shown to people without a preference of their own (ADM-016): by the
/// interface language, or one of three explicit patterns.
pub static DEFAULT_DATE_FORMAT: Setting<DateFormat> = Setting {
    key: "default_date_format",
    default: || DateFormat::Locale,
    validate: Ok,
    secret: false,
};

/// Every setting, in the order the admin UI lists them.
pub static ALL: &[&dyn Definition] = &[
    &BASE_URL,
    &SYSTEM_LANGUAGE,
    &DEFAULT_TIME_ZONE,
    &DEFAULT_HOME,
    &DEFAULT_DATE_FORMAT,
    &API_ALLOWED_ORIGINS,
    &UPLOAD_MAX_BYTES,
    &SESSION_IDLE_MINUTES,
    &REMEMBER_ME_DAYS,
    &SMTP_URL,
    &SMTP_USERNAME,
    &SMTP_PASSWORD,
    &MAIL_FROM,
    &MAIL_REPLY_TO,
    &EMAIL_ENABLED,
];

/// Languages prostor speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    En,
    Ru,
}

/// The pages a person may land on after sign-in (UI-004); the same strings as
/// `preferences::Home`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Home {
    Issues,
    Projects,
    Boards,
}

/// One of the four ways dates and times are shown (ADM-016). `Locale` follows the interface
/// language; the others are `31.12.2000, 23:59`, `31 дек. 2000, 23:59` (the month per the
/// viewer's language) and `2000-12-31 23:59`, each with the date-only, without-year and
/// day-month derivatives the web renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    Locale,
    DmyDot,
    DmyName,
    Iso,
}

// ---- Instance-wide flags (ADM-023) ----

/// An instance-wide capability flag (ADM-023): a boolean under `flag.<name>` in the settings
/// store, changed through the same prepare/audit/notify flow and applied without a restart.
/// The production list is empty in P1 — nothing is optional instance-wide yet; ADM-024–ADM-026
/// fill it in P2. Group-scoped capabilities are a separate mechanism (ACC-064, done).
#[derive(Debug, Clone, Copy)]
pub struct Flag {
    /// Bare name; the stored key is `flag.<name>`.
    pub name: &'static str,
    /// One line for the capabilities screen.
    pub description: &'static str,
    /// The value when no row is stored.
    pub default: bool,
    /// Whether an administrator may switch the flag off.
    pub may_switch_off: bool,
}

impl Flag {
    /// The stored key of this flag.
    pub fn key(&self) -> String {
        format!("flag.{}", self.name)
    }
}

/// Every instance-wide flag (ADM-023), in the order the capabilities screen lists them.
pub static FLAGS: &[Flag] = &[];

/// The session lifetimes of these settings.
pub fn session_policy(snapshot: &Snapshot) -> SessionPolicy {
    SessionPolicy {
        idle: Duration::minutes(i64::from(snapshot.get(&SESSION_IDLE_MINUTES))),
        remember: Duration::days(i64::from(snapshot.get(&REMEMBER_ME_DAYS))),
    }
}

fn in_range(value: u32, min: u32, max: u32) -> Result<u32, String> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(format!("must be from {min} to {max}"))
    }
}

/// Accepts names of the IANA time zone database, in any letter case; stores the database's
/// spelling.
fn validate_time_zone(name: String) -> Result<String, String> {
    let name = name.trim();
    let zone = jiff::tz::db()
        .get(name)
        .map_err(|_| format!("{name:?} is not a time zone of the IANA database"))?;
    Ok(zone.iana_name().unwrap_or(name).to_owned())
}

/// An email address with an optional name; empty means "not set" (NTF-006). The real shape is
/// checked when a mailer is built from the whole set of mail settings.
fn validate_address(value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.contains('@') {
        Ok(value)
    } else {
        Err("must be an email address, like `prostor <tracker@example.com>`".to_owned())
    }
}

/// Looks a setting up by key.
pub fn find(key: &str) -> Option<&'static dyn Definition> {
    ALL.iter().copied().find(|setting| setting.key() == key)
}

/// One origin of the API list: `scheme://host[:port]`, nothing more, stored without the trailing
/// slash. Reuses [`validate_base_url`]'s shape rules; the message says "origin" for the screen.
fn validate_origin(raw: String) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("an empty line is not an origin".to_owned());
    }
    let url = url::Url::parse(trimmed).map_err(|error| format!("not an origin: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("an origin starts with https:// or http://".to_owned());
    }
    if url.host().is_none() {
        return Err("an origin names a host".to_owned());
    }
    if url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(format!(
            "{trimmed:?} is an origin — scheme, host and at most a port, no path"
        ));
    }
    Ok(trimmed.to_owned())
}

/// The origins other sites may call the API from (INT-056): at most 32, each a plain origin,
/// stored once each in the order given.
fn validate_origins(origins: Vec<String>) -> Result<Vec<String>, String> {
    if origins.len() > 32 {
        return Err("at most 32 origins".to_owned());
    }
    let mut seen = HashSet::new();
    let mut clean = Vec::with_capacity(origins.len());
    for origin in origins {
        let origin = validate_origin(origin)?;
        if seen.insert(origin.clone()) {
            clean.push(origin);
        }
    }
    Ok(clean)
}

fn validate_base_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = value else { return Ok(None) };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let url = url::Url::parse(raw).map_err(|error| format!("not an absolute URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("must start with https:// or http://".to_owned());
    }
    if url.host().is_none() {
        return Err("must name a host".to_owned());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("must not have a query or a fragment".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not contain credentials".to_owned());
    }
    // One canonical form: links are built as `{base_url}/path`.
    Ok(Some(url.as_str().trim_end_matches('/').to_owned()))
}

// ---- Changes and snapshots ----

/// A validated change of one setting, ready to store. Only [`prepare`] and [`prepare_flag`]
/// create it.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    key: String,
    value: Option<Value>,
}

impl Change {
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The normalized value to store; `None` returns the setting to its default.
    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }
}

/// Validates a new value for `key`; `None` means "back to the default".
pub fn prepare(key: &str, value: Option<&Value>) -> Result<Change, SettingError> {
    let setting = find(key).ok_or_else(|| SettingError::Unknown(key.to_owned()))?;
    let value = value.map(|value| setting.parse(value)).transpose()?;
    // Storing the default is the same as having no stored value.
    let value = value.filter(|value| *value != setting.default_json());
    Ok(Change {
        key: setting.key().to_owned(),
        value,
    })
}

/// Validates a change of an instance-wide flag (ADM-023); `None` means "back to the default".
/// Storing the default is the same as having no stored value, as with settings.
pub fn prepare_flag(flag: &Flag, value: Option<bool>) -> Result<Change, SettingError> {
    let value = value.filter(|value| *value != flag.default);
    Ok(Change {
        key: flag.key(),
        value: value.map(Value::Bool),
    })
}

/// Current values of all settings, defaults filled in. Cheap to read, rebuilt on every change.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    values: HashMap<&'static str, Value>,
    /// The keys with a stored row; the rest sit at their defaults.
    stored: HashSet<&'static str>,
    /// Stored instance flag rows (ADM-023): `flag.<name>` → value.
    flags: HashMap<String, bool>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            values: ALL
                .iter()
                .map(|setting| (setting.key(), setting.default_json()))
                .collect(),
            stored: HashSet::new(),
            flags: HashMap::new(),
        }
    }
}

impl Snapshot {
    /// Builds a snapshot from stored `(key, value)` rows. Unknown keys and invalid values are
    /// skipped and returned as errors, so one bad row never stops the server. Rows under
    /// `flag.<name>` are instance flags (ADM-023) and always boolean.
    pub fn from_stored(
        rows: impl IntoIterator<Item = (String, Value)>,
    ) -> (Self, Vec<SettingError>) {
        let mut snapshot = Self::default();
        let mut errors = Vec::new();
        for (key, value) in rows {
            if key.starts_with("flag.") {
                match value {
                    Value::Bool(value) => {
                        snapshot.flags.insert(key, value);
                    }
                    value => errors.push(SettingError::WrongType {
                        key,
                        message: format!("a flag value is true or false, not {value:?}"),
                    }),
                }
                continue;
            }
            let Some(setting) = find(&key) else {
                errors.push(SettingError::Unknown(key));
                continue;
            };
            match setting.parse(&value) {
                Ok(value) => {
                    snapshot.values.insert(setting.key(), value);
                    snapshot.stored.insert(setting.key());
                }
                Err(error) => errors.push(error),
            }
        }
        (snapshot, errors)
    }

    /// Whether the setting has a stored row (a value that differs from its default).
    pub fn is_stored(&self, key: &str) -> bool {
        self.stored.contains(key)
    }

    /// The current value of a setting as JSON: the stored row, else the default. `None` for a
    /// key no setting declares.
    pub fn json(&self, key: &str) -> Option<Value> {
        self.values.get(key).cloned()
    }

    /// The current value of a setting.
    pub fn get<T: DeserializeOwned>(&self, setting: &Setting<T>) -> T {
        self.values
            .get(setting.key)
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_else(setting.default)
    }

    /// The effective value of an instance-wide flag (ADM-023): the stored row, else the flag's
    /// declared default.
    pub fn flag(&self, flag: &Flag) -> bool {
        self.flags.get(&flag.key()).copied().unwrap_or(flag.default)
    }

    /// Whether the flag has a stored row.
    pub fn flag_is_stored(&self, flag: &Flag) -> bool {
        self.flags.contains_key(&flag.key())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::*;

    #[test]
    fn registry_keys_are_unique_and_defaults_are_valid() {
        let mut keys = HashSet::new();
        for setting in ALL {
            assert!(
                keys.insert(setting.key()),
                "duplicate key {}",
                setting.key()
            );
            assert_eq!(
                setting.parse(&setting.default_json()).as_ref(),
                Ok(&setting.default_json()),
                "{}",
                setting.key()
            );
        }
    }

    #[test]
    fn snapshot_starts_with_defaults() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.get(&UPLOAD_MAX_BYTES), 10 * 1024 * 1024);
        assert_eq!(snapshot.get(&BASE_URL), None);
    }

    #[test]
    fn prepare_validates_and_normalizes() {
        let change = prepare("base_url", Some(&json!(" https://tracker.example.com/ "))).unwrap();
        assert_eq!(change.key(), "base_url");
        assert_eq!(change.value(), Some(&json!("https://tracker.example.com")));

        let change = prepare("upload_max_bytes", Some(&json!(1024))).unwrap();
        assert_eq!(change.value(), Some(&json!(1024)));
    }

    #[test]
    fn prepare_turns_the_default_into_a_reset() {
        assert_eq!(prepare("base_url", Some(&json!(""))).unwrap().value(), None);
        assert_eq!(
            prepare("upload_max_bytes", Some(&json!(10 * 1024 * 1024)))
                .unwrap()
                .value(),
            None
        );
        assert_eq!(prepare("upload_max_bytes", None).unwrap().value(), None);
    }

    #[test]
    fn prepare_rejects_bad_values() {
        assert!(matches!(
            prepare("no_such_setting", Some(&json!(1))),
            Err(SettingError::Unknown(_))
        ));
        assert!(matches!(
            prepare("upload_max_bytes", Some(&json!("ten"))),
            Err(SettingError::WrongType { .. })
        ));
        assert!(matches!(
            prepare("upload_max_bytes", Some(&json!(-1))),
            Err(SettingError::WrongType { .. })
        ));
        for bad in [
            json!(0),
            json!("ftp://example.com"),
            json!("example.com"),
            json!("https://example.com/?a=1"),
            json!("https://user:secret@example.com"),
        ] {
            let key = if bad.is_number() {
                "upload_max_bytes"
            } else {
                "base_url"
            };
            assert!(
                matches!(prepare(key, Some(&bad)), Err(SettingError::Invalid { .. })),
                "{bad}"
            );
        }
    }

    #[test]
    fn time_zones_come_from_the_iana_database() {
        let change = prepare("default_time_zone", Some(&json!(" europe/moscow "))).unwrap();
        assert_eq!(change.value(), Some(&json!("Europe/Moscow")));
        // Old names stay valid: browsers still report some of them.
        assert!(prepare("default_time_zone", Some(&json!("Europe/Kiev"))).is_ok());
        assert!(matches!(
            prepare("default_time_zone", Some(&json!("Mars/Olympus"))),
            Err(SettingError::Invalid { .. })
        ));
    }

    #[test]
    fn auth_004_session_lifetimes_have_bounds() {
        let snapshot = Snapshot::default();
        assert_eq!(
            session_policy(&snapshot),
            SessionPolicy {
                idle: Duration::hours(1),
                remember: Duration::days(30),
            }
        );
        for (key, bad) in [
            ("session_idle_minutes", 9),
            ("session_idle_minutes", 721),
            ("remember_me_days", 0),
            ("remember_me_days", 366),
        ] {
            assert!(
                matches!(
                    prepare(key, Some(&json!(bad))),
                    Err(SettingError::Invalid { .. })
                ),
                "{key} = {bad}"
            );
        }
        assert!(prepare("session_idle_minutes", Some(&json!(10))).is_ok());
        assert!(prepare("session_idle_minutes", Some(&json!(720))).is_ok());
    }

    #[test]
    fn system_language_is_a_known_language() {
        let change = prepare("system_language", Some(&json!("ru"))).unwrap();
        assert_eq!(change.value(), Some(&json!("ru")));
        assert!(matches!(
            prepare("system_language", Some(&json!("de"))),
            Err(SettingError::WrongType { .. })
        ));
    }

    #[test]
    fn snapshot_skips_unknown_and_invalid_rows() {
        let (snapshot, errors) = Snapshot::from_stored([
            ("upload_max_bytes".to_owned(), json!(2048)),
            ("retired_setting".to_owned(), json!(true)),
            ("base_url".to_owned(), json!("not a url")),
        ]);
        assert_eq!(snapshot.get(&UPLOAD_MAX_BYTES), 2048);
        assert_eq!(snapshot.get(&BASE_URL), None);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn mail_settings_know_their_secrets_and_defaults() {
        for setting in ALL {
            let expected = matches!(
                setting.key(),
                "smtp_url" | "smtp_username" | "smtp_password"
            );
            assert_eq!(setting.secret(), expected, "{}", setting.key());
        }
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.get(&SMTP_URL), "");
        assert_eq!(snapshot.get(&MAIL_FROM), "");
        assert!(!snapshot.get(&EMAIL_ENABLED));
        assert!(!snapshot.is_stored("smtp_url"));

        let change = prepare("mail_from", Some(&json!(" t@example.com "))).unwrap();
        assert_eq!(change.value(), Some(&json!("t@example.com")));
        let change = prepare("mail_reply_to", Some(&json!(""))).unwrap();
        assert_eq!(change.value(), None);
        assert!(matches!(
            prepare("mail_from", Some(&json!("tracker.example.com"))),
            Err(SettingError::Invalid { .. })
        ));
        assert_eq!(
            prepare("email_enabled", Some(&json!(false)))
                .unwrap()
                .value(),
            None
        );
    }

    #[test]
    fn a_snapshot_remembers_which_rows_are_stored() {
        let (snapshot, _) = Snapshot::from_stored([
            ("smtp_url".to_owned(), json!("smtp://127.0.0.1:1025")),
            ("email_enabled".to_owned(), json!(true)),
        ]);
        assert!(snapshot.is_stored("smtp_url"));
        assert!(snapshot.is_stored("email_enabled"));
        assert!(!snapshot.is_stored("mail_from"));
        assert_eq!(snapshot.get(&SMTP_URL), "smtp://127.0.0.1:1025");
        assert!(snapshot.get(&EMAIL_ENABLED));
    }

    #[test]
    fn ui_004_default_home_is_issues() {
        // Both layers agree: the registry and the personal preference default.
        assert_eq!(Snapshot::default().get(&DEFAULT_HOME), Home::Issues);
        assert_eq!(
            crate::preferences::Home::default(),
            crate::preferences::Home::Issues
        );
        let change = prepare("default_home", Some(&json!("boards"))).unwrap();
        assert_eq!(change.value(), Some(&json!("boards")));
        // Storing the default is the same as having no stored value.
        assert_eq!(
            prepare("default_home", Some(&json!("issues")))
                .unwrap()
                .value(),
            None
        );
        assert!(matches!(
            prepare("default_home", Some(&json!("dashboards"))),
            Err(SettingError::WrongType { .. })
        ));
    }

    #[test]
    fn adm_016_date_format_is_one_of_four() {
        assert_eq!(
            Snapshot::default().get(&DEFAULT_DATE_FORMAT),
            DateFormat::Locale
        );
        for value in ["dmy_dot", "dmy_name", "iso"] {
            assert!(
                prepare("default_date_format", Some(&json!(value))).is_ok(),
                "{value}"
            );
        }
        assert!(matches!(
            prepare("default_date_format", Some(&json!("31.12.2000"))),
            Err(SettingError::WrongType { .. })
        ));
    }

    #[test]
    fn int_056_api_origins_are_plain_and_few() {
        // Empty by default: same-origin only, and storing an empty list stores nothing.
        assert_eq!(
            Snapshot::default().get(&API_ALLOWED_ORIGINS),
            Vec::<String>::new()
        );
        assert_eq!(
            prepare("api.allowed_origins", Some(&json!([])))
                .unwrap()
                .value(),
            None
        );
        // Origins are trimmed, slashed off and kept once each, in the order given.
        let change = prepare(
            "api.allowed_origins",
            Some(&json!([
                "https://app.example.com",
                " https://app.example.com/ ",
                "http://localhost:5173"
            ])),
        )
        .unwrap();
        assert_eq!(
            change.value(),
            Some(&json!(["https://app.example.com", "http://localhost:5173"]))
        );
        for bad in [
            "app.example.com",
            "https://app.example.com/x",
            "ftp://app.example.com",
            "https://user:pass@app.example.com",
            "https://app.example.com/?a=1",
            "",
        ] {
            assert!(
                matches!(
                    prepare("api.allowed_origins", Some(&json!([bad]))),
                    Err(SettingError::Invalid { .. })
                ),
                "{bad:?}"
            );
        }
        let many: Vec<String> = (0..33)
            .map(|i| format!("https://host{i}.example.com"))
            .collect();
        assert!(matches!(
            prepare("api.allowed_origins", Some(&json!(many))),
            Err(SettingError::Invalid { .. })
        ));
    }

    #[test]
    fn adm_023_flags_are_boolean_rows() {
        static TEST: Flag = Flag {
            name: "test_flag",
            description: "A flag of the tests",
            default: true,
            may_switch_off: true,
        };
        assert_eq!(TEST.key(), "flag.test_flag");
        // Without a row the declared default applies; storing it stores nothing.
        let snapshot = Snapshot::default();
        assert!(snapshot.flag(&TEST));
        assert!(!snapshot.flag_is_stored(&TEST));
        assert_eq!(prepare_flag(&TEST, Some(true)).unwrap().value(), None);
        let change = prepare_flag(&TEST, Some(false)).unwrap();
        assert_eq!(change.key(), "flag.test_flag");
        assert_eq!(change.value(), Some(&Value::Bool(false)));
        assert_eq!(prepare_flag(&TEST, None).unwrap().value(), None);
        // A snapshot reads the stored row back.
        let (snapshot, errors) =
            Snapshot::from_stored([("flag.test_flag".to_owned(), json!(false))]);
        assert!(errors.is_empty());
        assert!(!snapshot.flag(&TEST));
        assert!(snapshot.flag_is_stored(&TEST));
        // A flag row of the wrong type is skipped, not fatal.
        let (_, errors) = Snapshot::from_stored([("flag.test_flag".to_owned(), json!("yes"))]);
        assert!(matches!(errors[..], [SettingError::WrongType { .. }]));
    }
}
