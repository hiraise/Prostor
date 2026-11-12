//! Changing instance-wide settings (ADM-002): validated by the domain registry, stored with an
//! audit event and a notification that makes every node reload, in one transaction. Secret
//! settings (NTF-004) are sealed into their own table and never appear in an audit event. The
//! general admin screen reads them through [`admin_view`] and saves change sets through
//! [`save`] (plan step 1.11 §3.1); instance-wide flags (ADM-023, §3.2) go through
//! [`set_flag`].

use serde_json::{Value, json};
use sqlx::PgConnection;
use trackflow_domain::Actor;
use trackflow_domain::access::Access;
use trackflow_domain::access::permission::Permission;
use trackflow_domain::secret::Key;
use trackflow_domain::settings::{self, Change, Definition, SettingError, Snapshot};
use trackflow_store::PgPool;
use trackflow_store::{audit, bus, settings as store};

/// The keys that belong to the mail screen and its API («Почта», 1.10c), not to the general
/// settings screen: they never appear in [`admin_view`] and a change set refuses them.
const MAIL_KEYS: [&str; 6] = [
    "smtp_url",
    "smtp_username",
    "smtp_password",
    "mail_from",
    "mail_reply_to",
    "email_enabled",
];

/// Why a setting was not changed.
#[derive(Debug, thiserror::Error)]
pub enum SetError {
    #[error(transparent)]
    Invalid(#[from] SettingError),
    #[error("{0}")]
    Refused(std::borrow::Cow<'static, str>),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<SettingError> for crate::Error {
    fn from(error: SettingError) -> Self {
        crate::Error::from(SetError::Invalid(error))
    }
}

impl From<SetError> for crate::Error {
    fn from(error: SetError) -> Self {
        match error {
            SetError::Invalid(SettingError::Unknown(key)) => {
                crate::Error::Internal(format!("no setting {key:?}"))
            }
            SetError::Invalid(
                SettingError::WrongType { key, message } | SettingError::Invalid { key, message },
            ) => crate::Error::field(
                &key,
                trackflow_domain::validation::Problem::Invalid,
                message,
            ),
            SetError::Refused(message) => crate::Error::Refused(message),
            SetError::Database(error) => crate::Error::Database(error),
        }
    }
}

/// Changes a setting (`None` restores the default). Returns `false` when the value was already
/// the same.
pub async fn set(
    pool: &PgPool,
    key: &str,
    value: Option<&Value>,
    actor: Actor,
    secret_key: Option<&Key>,
) -> Result<bool, SetError> {
    let change = settings::prepare(key, value)?;
    let mut tx = pool.begin().await?;
    let changed = apply(&mut tx, &change, actor, secret_key).await?;
    tx.commit().await?;
    Ok(changed)
}

/// Changes an instance-wide flag (ADM-023): the same flow as a setting — one transaction, an
/// audit event, a notification that makes every node reload. `None` restores the default.
pub async fn set_flag(
    pool: &PgPool,
    flag: &settings::Flag,
    value: Option<bool>,
    actor: Actor,
) -> Result<bool, SetError> {
    let change = settings::prepare_flag(flag, value)?;
    let mut tx = pool.begin().await?;
    let changed = apply(&mut tx, &change, actor, None).await?;
    tx.commit().await?;
    Ok(changed)
}

// ---- The general admin screen (plan step 1.11 §3.1–§3.2) ----

/// One setting of the screen: what it is now, where it falls back to, whether a row is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingView {
    pub key: String,
    pub value: Value,
    pub default: Value,
    pub is_stored: bool,
}

/// One instance-wide flag of the capabilities screen (ADM-023).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagView {
    pub key: String,
    pub description: String,
    pub value: bool,
    pub default: bool,
    pub is_stored: bool,
    pub may_switch_off: bool,
}

/// The settings of the «Настройки» screen: every non-secret setting except the mail group, plus
/// the instance-wide flags. Needs `system.read`.
pub fn admin_view(actor: &Access, snapshot: &Snapshot) -> Result<AdminSettingsView, crate::Error> {
    actor.require(Permission::SystemRead)?;
    Ok(AdminSettingsView {
        settings: settings::ALL
            .iter()
            .filter(|setting| !setting.secret() && !MAIL_KEYS.contains(&setting.key()))
            .map(|setting| SettingView {
                key: setting.key().to_owned(),
                value: snapshot
                    .json(setting.key())
                    .unwrap_or_else(|| setting.default_json()),
                default: setting.default_json(),
                is_stored: snapshot.is_stored(setting.key()),
            })
            .collect(),
        flags: settings::FLAGS
            .iter()
            .map(|flag| FlagView {
                key: flag.name.to_owned(),
                description: flag.description.to_owned(),
                value: snapshot.flag(flag),
                default: flag.default,
                is_stored: snapshot.flag_is_stored(flag),
                may_switch_off: flag.may_switch_off,
            })
            .collect(),
    })
}

/// What the «Настройки» screen reads: the settings and the flags, without the mail group.
#[derive(Debug, Clone, PartialEq)]
pub struct AdminSettingsView {
    pub settings: Vec<SettingView>,
    pub flags: Vec<FlagView>,
}

/// Saves a change set of the general settings (ADM-002): each key goes through the registry, one
/// transaction, one audit event per key, every node reloads. A mail key or an unknown key is a
/// problem on that key, not a change. Needs `system.write`.
pub async fn save(
    pool: &PgPool,
    actor: &Access,
    changes: &[(String, Option<Value>)],
    secret_key: Option<&Key>,
) -> Result<(), crate::Error> {
    actor.require(Permission::SystemWrite)?;
    let mut prepared = Vec::with_capacity(changes.len());
    for (key, value) in changes {
        if MAIL_KEYS.contains(&key.as_str()) {
            return Err(crate::Error::Refused(
                "the mail settings have their own screen and API (Администрирование → Почта)"
                    .into(),
            ));
        }
        let change = if let Some(flag) = settings::FLAGS.iter().find(|f| &f.key() == key) {
            let value = value
                .as_ref()
                .map(|value| {
                    value.as_bool().ok_or_else(|| SettingError::WrongType {
                        key: key.clone(),
                        message: "a flag value is true or false".to_owned(),
                    })
                })
                .transpose()?;
            settings::prepare_flag(flag, value)?
        } else if settings::find(key).is_none() {
            return Err(crate::Error::field(
                key,
                trackflow_domain::validation::Problem::Invalid,
                "no such setting",
            ));
        } else {
            settings::prepare(key, value.as_ref())?
        };
        prepared.push(change);
    }
    let mut tx = pool.begin().await?;
    for change in prepared {
        apply(&mut tx, &change, Actor::User(actor.user()), secret_key).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Stores a validated change inside the caller's transaction, with its audit event and
/// notification. Returns `false` when the value was already the same.
pub async fn apply(
    conn: &mut PgConnection,
    change: &Change,
    actor: Actor,
    secret_key: Option<&Key>,
) -> Result<bool, SetError> {
    let key = change.key();
    let secret = settings::find(key).is_some_and(Definition::secret);
    if secret && change.value().is_some() && secret_key.is_none() {
        return Err(SetError::Refused(
            "this instance has no TRACKFLOW_SECRET_KEY: set one (32 bytes, base64) to store secrets"
                .into(),
        ));
    }
    let old = if secret {
        read_secret(conn, key, secret_key).await?
    } else {
        store::lock_value(conn, key).await?
    };
    // Without the key a stored secret reads as its default; a reset still removes the row.
    let stored = if secret {
        store::secret_exists(conn, key).await?
    } else {
        old.is_some()
    };
    if (!stored && change.value().is_none()) || old.as_ref() == change.value() {
        return Ok(false);
    }
    let details = if secret {
        // NTF-004: the audit trail records that a secret changed, never its value.
        json!({ "secret": true })
    } else {
        // `null` stands for the default on either side.
        json!({ "old": old, "new": change.value() })
    };
    match change.value() {
        Some(value) if secret => {
            // The refusal above leaves exactly one shape here: a key to seal with.
            let Some(secret_key) = secret_key else {
                return Err(SetError::Refused("no key to seal a secret with".into()));
            };
            let plain = value.as_str().ok_or_else(|| {
                SetError::Invalid(SettingError::WrongType {
                    key: key.to_owned(),
                    message: "a secret value must be a string".to_owned(),
                })
            })?;
            let sealed = secret_key.seal(key, plain);
            if sealed.is_empty() {
                return Err(SetError::Refused("no secure random source".into()));
            }
            store::secret_upsert(conn, key, &sealed).await?;
        }
        Some(value) => store::upsert(conn, key, value, actor).await?,
        None if secret => store::secret_remove(conn, key).await?,
        None => store::remove(conn, key).await?,
    }
    audit::record(
        conn,
        actor,
        "setting.changed",
        &format!("setting:{key}"),
        details,
    )
    .await?;
    bus::notify(conn, bus::SETTINGS, key).await?;
    Ok(true)
}

/// The stored value of a secret setting, decrypted; `None` means the default. Without a key the
/// answer is the default, so a save that keeps a secret sees it as unchanged and skips it.
async fn read_secret(
    conn: &mut PgConnection,
    key: &str,
    secret_key: Option<&Key>,
) -> Result<Option<Value>, SetError> {
    Ok(match secret_key {
        Some(secret_key) => store::secret_value(conn, secret_key, key).await?,
        None => None,
    })
}
