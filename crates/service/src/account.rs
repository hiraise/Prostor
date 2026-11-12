//! A signed-in person's own account: who they are and their password (AUTH-009).

use serde_json::json;
use trackflow_domain::UserId;
use trackflow_domain::accounts::check_new_password;
use trackflow_domain::auth::Principal;
use trackflow_domain::validation::{Problem, Problems, Rejection};
use trackflow_store::users::{self, User};
use trackflow_store::{PgPool, audit, sessions};

use crate::passwords::{Passwords, Verified};
use crate::{Error, Result};

/// The caller's own account.
pub async fn me(pool: &PgPool, user: UserId) -> Result<User> {
    users::get(pool, user)
        .await?
        .ok_or(Error::NotFound("the account no longer exists"))
}

/// Changes the caller's password after checking the current one. Every other session of the
/// account ends, so a stolen session does not outlive the change; API tokens keep working.
/// Callers make sure `principal` is a session: tokens never change credentials.
pub async fn change_password(
    pool: &PgPool,
    passwords: &Passwords,
    principal: &Principal,
    current: &str,
    new: &str,
) -> Result<()> {
    let mut problems = Problems::new();
    if current.is_empty() {
        problems.add(
            "current_password",
            Rejection::new(Problem::Required, "the current password is required"),
        );
    }
    problems.check("new_password", check_new_password(new));
    problems.finish().map_err(Error::Invalid)?;

    let mut conn = pool.acquire().await?;
    let stored = users::lock_password(&mut conn, principal.user).await?;
    drop(conn);
    if !matches!(
        passwords.verify(current, stored.as_deref()).await?,
        Verified::Match { .. }
    ) {
        return Err(Error::field(
            "current_password",
            Problem::Incorrect,
            "the current password is not correct",
        ));
    }
    let hash = passwords.hash(new).await?;

    let mut tx = pool.begin().await?;
    // Changed by another request while this one was hashing: let the person try again.
    if users::lock_password(&mut tx, principal.user).await? != stored {
        return Err(Error::Conflict("the password was changed at the same time"));
    }
    // Choosing a new password satisfies a required change (AUTH-010).
    users::set_password(&mut tx, principal.user, &hash, false).await?;
    let ended = sessions::delete_for_user(&mut tx, principal.user, principal.session_id()).await?;
    audit::record(
        &mut tx,
        principal.actor(),
        "user.password_changed",
        &format!("user:{}", principal.user),
        json!({ "sessions_ended": ended }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
