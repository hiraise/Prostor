//! The first-run setup (ADM-047): the first visitor proves access to the server with the code
//! from its log, creates the administrator, chooses the language, address and time zone, and is
//! signed in. Also creates the disabled guest account (ACC-001). Runs once.

use serde_json::{Value, json};
use taskmaster_domain::Actor;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::accounts::{Email, FullName, Username, check_new_password};
use taskmaster_domain::auth::codes;
use taskmaster_domain::settings::{
    self, BASE_URL, Change, DEFAULT_TIME_ZONE, Language, SYSTEM_LANGUAGE, SettingError,
};
use taskmaster_domain::validation::{Problem, Problems, Rejection};
use taskmaster_store::access::{self as assignments, Holder};
use taskmaster_store::instance::{self, SetupState};
use taskmaster_store::sessions::Origin;
use taskmaster_store::users::{self, Builtin, NewUser};
use taskmaster_store::{PgPool, audit, groups, roles};
use time::OffsetDateTime;

use crate::auth::{SignedIn, start_session};
use crate::passwords::Passwords;
use crate::secrets;
use crate::{Error, Result};

/// What the setup wizard sends.
#[derive(Clone)]
pub struct Setup {
    /// The code from the server log.
    pub code: String,
    pub username: String,
    pub full_name: String,
    pub email: String,
    pub password: String,
    /// `en` or `ru`.
    pub system_language: String,
    /// Prefilled with the address the browser used.
    pub base_url: Option<String>,
    /// An IANA time zone, prefilled with the browser's.
    pub time_zone: String,
}

/// Makes sure a pending setup has a code and returns the state: the code to print in the log, or
/// that the setup is complete. `configured` comes from `TASKMASTER_SETUP_CODE`.
pub async fn prepare(pool: &PgPool, configured: Option<&str>) -> Result<SetupState> {
    let generated = secrets::setup_code()?;
    Ok(instance::prepare_setup(pool, &generated, configured).await?)
}

pub async fn is_complete(pool: &PgPool) -> Result<bool> {
    Ok(instance::setup_complete(pool).await?)
}

/// Runs the setup and signs the new administrator in.
pub async fn run(
    pool: &PgPool,
    passwords: &Passwords,
    setup: Setup,
    now: OffsetDateTime,
    origin: &Origin,
) -> Result<SignedIn> {
    let mut problems = Problems::new();
    let username = problems.check("username", Username::parse(&setup.username));
    let full_name = problems.check("full_name", FullName::parse(&setup.full_name));
    let email = problems.check("email", Email::parse(&setup.email));
    problems.check("password", check_new_password(&setup.password));
    let language = setting(
        &mut problems,
        "system_language",
        SYSTEM_LANGUAGE.key,
        json!(setup.system_language),
    );
    let base_url = setting(
        &mut problems,
        "base_url",
        BASE_URL.key,
        json!(setup.base_url),
    );
    let time_zone = setting(
        &mut problems,
        "time_zone",
        DEFAULT_TIME_ZONE.key,
        json!(setup.time_zone),
    );

    // The code is checked before the slow password hash, and again under the lock.
    let mut conn = pool.acquire().await?;
    let Some(code) = instance::lock_pending_setup(&mut conn).await? else {
        return Err(Error::Conflict("the setup is already complete"));
    };
    drop(conn);
    if !codes::matches(&setup.code, &code) {
        problems.add(
            "code",
            Rejection::new(Problem::Incorrect, "the setup code is not correct"),
        );
    }
    let mut problems = problems.into_vec();
    if !problems.is_empty() {
        // Field order in the form: the code first.
        problems.sort_by_key(|problem| problem.path != "code");
        return Err(Error::Invalid(problems));
    }
    let (Some(username), Some(full_name), Some(email)) = (username, full_name, email) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    let hash = passwords.hash(&setup.password).await?;

    let mut tx = pool.begin().await?;
    match instance::lock_pending_setup(&mut tx).await? {
        Some(current) if current == code => {}
        Some(_) => {
            return Err(Error::field(
                "code",
                Problem::Incorrect,
                "the setup code has changed",
            ));
        }
        None => return Err(Error::Conflict("the setup is already complete")),
    }
    let admin = users::insert(
        &mut tx,
        &NewUser {
            username: username.as_str(),
            full_name: full_name.as_str(),
            email: Some(email.as_str()),
            builtin: Some(Builtin::Admin),
            disabled: false,
            invited: false,
        },
    )
    .await?;
    users::set_password(&mut tx, admin, &hash, false).await?;
    // The address is the administrator's own, typed with the code from the server log.
    users::verify_email(&mut tx, admin, email.as_str()).await?;
    let role = roles::builtin_id(&mut *tx, PredefinedRole::SystemAdmin.key())
        .await?
        .ok_or(Error::Internal(
            "the system administrator role is missing".to_owned(),
        ))?;
    assignments::insert(&mut tx, role, Holder::User(admin), None, None).await?;
    groups::join_auto(&mut tx, admin).await?;
    let guest = users::insert(
        &mut tx,
        &NewUser {
            username: Username::guest().as_str(),
            full_name: "Guest",
            email: None,
            builtin: Some(Builtin::Guest),
            disabled: true,
            invited: false,
        },
    )
    .await?;
    let actor = Actor::User(admin);
    for change in [language, base_url, time_zone].into_iter().flatten() {
        crate::settings::apply(&mut tx, &change, actor, None).await?;
    }
    audit::record(
        &mut tx,
        actor,
        "instance.setup_completed",
        "instance",
        json!({ "admin": admin, "guest": guest }),
    )
    .await?;
    instance::complete_setup(&mut tx).await?;
    // The default field set speaks the chosen language (FLD-070).
    let chosen = serde_json::from_value(json!(setup.system_language)).unwrap_or(Language::En);
    crate::seed::default_fields(&mut tx, chosen).await?;
    crate::issue_links::default_types(&mut tx, chosen).await?;
    let (session, secret) = start_session(&mut tx, admin, false, now, origin).await?;
    tx.commit().await?;

    let user = users::get(pool, admin)
        .await?
        .ok_or(Error::NotFound("the new administrator is gone"))?;
    Ok(SignedIn {
        user,
        session,
        secret,
    })
}

/// Validates a setting value, recording a problem under `path`.
fn setting(problems: &mut Problems, path: &str, key: &str, value: Value) -> Option<Change> {
    match settings::prepare(key, Some(&value)) {
        Ok(change) => Some(change),
        Err(SettingError::Invalid { message, .. } | SettingError::WrongType { message, .. }) => {
            problems.add(path, Rejection::new(Problem::Invalid, message));
            None
        }
        Err(SettingError::Unknown(key)) => {
            problems.add(
                path,
                Rejection::new(Problem::Invalid, format!("no setting {key}")),
            );
            None
        }
    }
}
