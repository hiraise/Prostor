//! Emailed one-time links (AUTH-008, AUTH-014, ACC-004): choosing a new password after forgetting
//! it, confirming an address, and accepting an invitation. A link carries 256 random bits after
//! `#`; only their hash is stored, a newer link of the same purpose replaces the older one, and
//! using a link deletes it.

use prostor_domain::accounts::{FullName, Username, check_new_password};
use prostor_domain::auth::{LinkPurpose, RESET_RESEND_INTERVAL};
use prostor_domain::validation::{Problem, Problems};
use prostor_store::account_links::{self, Link};
use prostor_store::sessions::{self, Origin};
use prostor_store::users::{self, IdentityChange, User};
use prostor_store::{PgPool, audit};
use serde_json::json;
use sqlx::PgConnection;
use time::OffsetDateTime;

use crate::auth::{SignedIn, start_session};
use crate::mail::{self, Letters};
use crate::passwords::Passwords;
use crate::secrets::{self, Secret};
use crate::{Error, Result};

/// Creates a link for the account, replacing older ones of the purpose, inside the caller's
/// transaction. The secret goes into the email only.
pub(crate) async fn issue(
    conn: &mut PgConnection,
    user: &User,
    purpose: LinkPurpose,
    email: &str,
    now: OffsetDateTime,
) -> Result<Secret> {
    let secret = secrets::session()?;
    account_links::replace(
        conn,
        user.id,
        purpose.key(),
        email,
        &secret.hash,
        now,
        now + purpose.lifetime(),
    )
    .await?;
    Ok(secret)
}

/// Queues an email that confirms the account's current address (AUTH-014).
pub(crate) async fn send_verification(
    conn: &mut PgConnection,
    letters: &Letters,
    user: &User,
    email: &str,
    initiator: prostor_domain::Actor,
    now: OffsetDateTime,
) -> Result<()> {
    let base = letters.require()?;
    let secret = issue(conn, user, LinkPurpose::EmailVerification, email, now).await?;
    let message = mail::email_verification(letters, base, email, &user.username, &secret.value);
    mail::queue(conn, &message, initiator).await
}

/// What a link is for, shown before it is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkInfo {
    pub purpose: LinkPurpose,
    pub email: String,
    pub username: String,
    pub full_name: String,
    pub expired: bool,
}

/// Looks a link up without using it. Unknown and used links are "not found".
pub async fn inspect(pool: &PgPool, token: &str, now: OffsetDateTime) -> Result<LinkInfo> {
    let link = account_links::find_by_hash(pool, &secrets::hash(token.trim()))
        .await?
        .ok_or(Error::NotFound("the link is not valid"))?;
    let purpose =
        LinkPurpose::parse(&link.purpose).ok_or(Error::NotFound("the link is not valid"))?;
    let user = users::get(pool, link.user_id)
        .await?
        .filter(|user| !user.is_deleted())
        .ok_or(Error::NotFound("the link is not valid"))?;
    Ok(LinkInfo {
        purpose,
        email: link.email,
        username: user.username,
        full_name: user.full_name,
        expired: link.expires_at <= now,
    })
}

/// Sends a password-reset link to an address (AUTH-008). Always answers the same whether or not
/// an account has the address, so nobody learns who has one; the link goes only to a verified
/// address of an enabled account, and at most once a minute.
pub async fn request_password_reset(
    pool: &PgPool,
    letters: &Letters,
    email: &str,
    now: OffsetDateTime,
) -> Result<()> {
    let base = letters.require()?;
    let email = email.trim();
    if email.is_empty() {
        return Err(Error::field(
            "email",
            Problem::Required,
            "email is required",
        ));
    }
    let mut tx = pool.begin().await?;
    let Some(found) = users::find_by_email(&mut tx, email).await? else {
        return Ok(());
    };
    let Some(user) = users::lock(&mut tx, found.id).await? else {
        return Ok(());
    };
    let address = user.email.clone().unwrap_or_default();
    let eligible = !user.is_deleted()
        && !user.is_guest()
        && user.disabled_at.is_none()
        && user.email_verified_at.is_some()
        && address.eq_ignore_ascii_case(email);
    if !eligible {
        return Ok(());
    }
    let recent = account_links::latest(&mut tx, user.id, LinkPurpose::PasswordReset.key())
        .await?
        .is_some_and(|at| now - at < RESET_RESEND_INTERVAL);
    if recent {
        return Ok(());
    }
    let secret = issue(&mut tx, &user, LinkPurpose::PasswordReset, &address, now).await?;
    let message = mail::password_reset(letters, base, &address, &user.username, &secret.value);
    mail::queue(&mut tx, &message, prostor_domain::Actor::System).await?;
    audit::record(
        &mut tx,
        prostor_domain::Actor::System,
        "user.password_reset_requested",
        &format!("user:{}", user.id),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Finds a usable link of the purpose and locks it with its account.
async fn take(
    conn: &mut PgConnection,
    token: &str,
    purpose: LinkPurpose,
    now: OffsetDateTime,
) -> Result<(Link, User)> {
    let link = account_links::lock_by_hash(conn, &secrets::hash(token.trim()))
        .await?
        .filter(|link| link.purpose == purpose.key())
        .ok_or(Error::NotFound("the link is not valid"))?;
    if link.expires_at <= now {
        return Err(Error::Conflict("the link has expired"));
    }
    let user = users::lock(conn, link.user_id)
        .await?
        .filter(|user| !user.is_deleted() && user.disabled_at.is_none())
        .ok_or(Error::NotFound("the link is not valid"))?;
    Ok((link, user))
}

/// Sets a new password from a reset link and signs in (AUTH-008). The link proves the address,
/// so it becomes verified; every other session of the account ends.
pub async fn reset_password(
    pool: &PgPool,
    passwords: &Passwords,
    token: &str,
    new_password: &str,
    now: OffsetDateTime,
    origin: &Origin,
) -> Result<SignedIn> {
    if let Err(rejection) = check_new_password(new_password) {
        return Err(Error::rejected("new_password", rejection));
    }
    let hash = passwords.hash(new_password).await?;
    let mut tx = pool.begin().await?;
    let (link, user) = take(&mut tx, token, LinkPurpose::PasswordReset, now).await?;
    users::set_password(&mut tx, user.id, &hash, false).await?;
    users::verify_email(&mut tx, user.id, &link.email).await?;
    account_links::delete_for_user(&mut tx, user.id, Some(LinkPurpose::PasswordReset.key()))
        .await?;
    let ended = sessions::delete_for_user(&mut tx, user.id, None).await?;
    audit::record(
        &mut tx,
        prostor_domain::Actor::User(user.id),
        "user.password_reset",
        &format!("user:{}", user.id),
        json!({ "sessions_ended": ended }),
    )
    .await?;
    let (session, secret) = start_session(&mut tx, user.id, false, now, origin).await?;
    tx.commit().await?;
    let user = reload(pool, user.id).await?;
    Ok(SignedIn {
        user,
        session,
        secret,
    })
}

/// What an invited person sends to join (ACC-004).
#[derive(Clone)]
pub struct Acceptance {
    pub password: String,
    /// Changes of the name and username the invitation started with.
    pub full_name: Option<String>,
    pub username: Option<String>,
}

/// Accepts an invitation: sets the password (and optionally a name and username), verifies the
/// address and signs in (ACC-004).
pub async fn accept_invitation(
    pool: &PgPool,
    passwords: &Passwords,
    token: &str,
    acceptance: Acceptance,
    now: OffsetDateTime,
    origin: &Origin,
) -> Result<SignedIn> {
    let mut problems = Problems::new();
    problems.check("password", check_new_password(&acceptance.password));
    let full_name = acceptance
        .full_name
        .as_deref()
        .and_then(|name| problems.check("full_name", FullName::parse(name)));
    let username = acceptance
        .username
        .as_deref()
        .and_then(|name| problems.check("username", Username::parse(name)));
    problems.finish().map_err(Error::Invalid)?;
    let hash = passwords.hash(&acceptance.password).await?;

    let mut tx = pool.begin().await?;
    let (link, user) = take(&mut tx, token, LinkPurpose::Invitation, now).await?;
    if user.invited_at.is_none() {
        return Err(Error::NotFound("the link is not valid"));
    }
    if let Some(username) = &username {
        let (taken, _) = users::taken(&mut tx, username.as_str(), "", Some(user.id)).await?;
        if taken {
            return Err(Error::field(
                "username",
                Problem::Taken,
                "the username is taken",
            ));
        }
    }
    users::update_identity(
        &mut tx,
        user.id,
        &IdentityChange {
            username: username.as_ref().map(Username::as_str),
            full_name: full_name.as_ref().map(FullName::as_str),
            email: None,
        },
    )
    .await?;
    users::set_password(&mut tx, user.id, &hash, false).await?;
    users::accept_invitation(&mut tx, user.id).await?;
    users::verify_email(&mut tx, user.id, &link.email).await?;
    account_links::delete_for_user(&mut tx, user.id, Some(LinkPurpose::Invitation.key())).await?;
    audit::record(
        &mut tx,
        prostor_domain::Actor::User(user.id),
        "user.invitation_accepted",
        &format!("user:{}", user.id),
        json!({}),
    )
    .await?;
    let (session, secret) = start_session(&mut tx, user.id, false, now, origin).await?;
    tx.commit().await?;
    let user = reload(pool, user.id).await?;
    Ok(SignedIn {
        user,
        session,
        secret,
    })
}

/// Confirms an address from a verification link (AUTH-014); returns it. A link for an address the
/// account no longer has is gone: it can confirm nothing any more.
pub async fn confirm_email(pool: &PgPool, token: &str, now: OffsetDateTime) -> Result<String> {
    let mut tx = pool.begin().await?;
    let (link, user) = take(&mut tx, token, LinkPurpose::EmailVerification, now).await?;
    account_links::delete(&mut tx, link.id).await?;
    if !users::verify_email(&mut tx, user.id, &link.email).await? {
        tx.commit().await?;
        return Err(Error::NotFound("the link is not valid"));
    }
    audit::record(
        &mut tx,
        prostor_domain::Actor::User(user.id),
        "user.email_verified",
        &format!("user:{}", user.id),
        json!({ "email": link.email }),
    )
    .await?;
    tx.commit().await?;
    Ok(link.email)
}

async fn reload(pool: &PgPool, id: prostor_domain::UserId) -> Result<User> {
    users::get(pool, id)
        .await?
        .ok_or(Error::NotFound("the account no longer exists"))
}
