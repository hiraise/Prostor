//! Administrator recovery from the server's command line (ADM-051, AUTH-073): for whoever runs
//! the server when nobody can sign in as an administrator. Every use is audited.

use serde_json::json;
use taskmaster_domain::Actor;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::accounts::{Email, FullName, Username};
use taskmaster_domain::validation::{Problem, Problems, Rejection};
use taskmaster_store::access::{self as assignments, Holder};
use taskmaster_store::users::{self, Builtin, NewUser, User};
use taskmaster_store::{PgPool, audit, bus, groups, instance, roles, sessions};

use crate::mail::{self, Letters};
use crate::passwords::Passwords;
use crate::secrets;
use crate::{Error, Result};

/// Whose access to restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The administrator made by the setup wizard.
    BuiltinAdmin,
    /// An existing account by username or email; it becomes an administrator.
    Login(String),
    /// A new administrator, for when no existing account should be used.
    New {
        username: String,
        email: String,
        full_name: Option<String>,
    },
}

/// What recovery did.
#[derive(Debug, Clone)]
pub struct Recovered {
    pub user: User,
    /// The new one-time password, for the operator to hand over.
    pub password: String,
    pub created: bool,
    pub sessions_ended: u64,
}

/// Gives the target a new random password, enables it, makes it a system administrator and ends
/// its sessions. With email on, the use also queues a report to the built-in administrator's
/// address (ADM-051), in the same transaction.
pub async fn recover(
    pool: &PgPool,
    passwords: &Passwords,
    target: Target,
    letters: &Letters,
) -> Result<Recovered> {
    if !instance::setup_complete(pool).await? {
        return Err(Error::Conflict(
            "the setup is not complete: open taskmaster in a browser and use the setup code from \
             the server log",
        ));
    }
    let new = match &target {
        Target::New {
            username,
            email,
            full_name,
        } => Some(validate_new(username, email, full_name.as_deref())?),
        Target::BuiltinAdmin | Target::Login(_) => None,
    };
    let password = secrets::password()?;
    let hash = passwords.hash(&password).await?;

    let mut tx = pool.begin().await?;
    let (id, created, username) = match (&target, new) {
        (Target::New { .. }, Some((username, email, full_name))) => {
            let (username_taken, email_taken) =
                users::taken(&mut tx, username.as_str(), email.as_str(), None).await?;
            let mut problems = Problems::new();
            if username_taken {
                problems.add(
                    "username",
                    Rejection::new(Problem::Taken, "the username is taken"),
                );
            }
            if email_taken {
                problems.add(
                    "email",
                    Rejection::new(Problem::Taken, "the email is taken"),
                );
            }
            problems.finish().map_err(Error::Invalid)?;
            let id = users::insert(
                &mut tx,
                &NewUser {
                    username: username.as_str(),
                    full_name: full_name.as_str(),
                    email: Some(email.as_str()),
                    builtin: None,
                    disabled: false,
                    invited: false,
                },
            )
            .await?;
            groups::join_auto(&mut tx, id).await?;
            (id, true, username.as_str().to_owned())
        }
        (Target::BuiltinAdmin, _) => {
            let user =
                users::lock_builtin(&mut tx, Builtin::Admin)
                    .await?
                    .ok_or(Error::NotFound(
                        "the built-in administrator no longer exists; create a new one",
                    ))?;
            (user.id, false, user.username)
        }
        (Target::Login(login), _) => {
            let user = users::lock_by_login(&mut tx, login.trim())
                .await?
                .ok_or(Error::NotFound("no account has this username or email"))?;
            if user.is(Builtin::Guest) {
                return Err(Error::Conflict(
                    "the guest account cannot become an administrator",
                ));
            }
            (user.id, false, user.username)
        }
        (Target::New { .. }, None) => {
            return Err(Error::Internal("validated values are missing".to_owned()));
        }
    };
    users::restore(&mut tx, id).await?;
    let role = roles::builtin_id(&mut *tx, PredefinedRole::SystemAdmin.key())
        .await?
        .ok_or(Error::Internal(
            "the system administrator role is missing".to_owned(),
        ))?;
    assignments::insert(&mut tx, role, Holder::User(id), None, None).await?;
    // The one-time password is the operator's; the person chooses their own at sign-in.
    users::set_password(&mut tx, id, &hash, true).await?;
    bus::notify(&mut tx, bus::ACCESS, "").await?;
    let sessions_ended = sessions::delete_for_user(&mut tx, id, None).await?;
    audit::record(
        &mut tx,
        Actor::System,
        if created {
            "user.created"
        } else {
            "user.recovered"
        },
        &format!("user:{id}"),
        json!({ "via": "recovery_command", "sessions_ended": sessions_ended }),
    )
    .await?;
    // With email on, the use also reports to the built-in administrator's address (ADM-051),
    // whoever was recovered: the operator sees the console, the owner sees the letter. No
    // address on that account, or no address to link to, leaves the audit row the only trace.
    if let (true, Some(base)) = (letters.enabled, letters.base_url.as_deref()) {
        let owner = users::lock_builtin(&mut tx, Builtin::Admin)
            .await?
            .and_then(|admin| admin.email);
        if let Some(owner) = owner.as_deref() {
            mail::queue(
                &mut tx,
                &mail::recovery_report(letters, base, owner, &username, created, sessions_ended),
                Actor::System,
            )
            .await?;
        }
    }
    tx.commit().await?;

    let user = users::get(pool, id)
        .await?
        .ok_or(Error::NotFound("the account is gone"))?;
    Ok(Recovered {
        user,
        password,
        created,
        sessions_ended,
    })
}

fn validate_new(
    username: &str,
    email: &str,
    full_name: Option<&str>,
) -> Result<(Username, Email, FullName)> {
    let mut problems = Problems::new();
    let username = problems.check("username", Username::parse(username));
    let email = problems.check("email", Email::parse(email));
    let full_name = problems.check(
        "full_name",
        FullName::parse(full_name.unwrap_or("Administrator")),
    );
    problems.finish().map_err(Error::Invalid)?;
    match (username, email, full_name) {
        (Some(username), Some(email), Some(full_name)) => Ok((username, email, full_name)),
        _ => Err(Error::Internal("validated values are missing".to_owned())),
    }
}
