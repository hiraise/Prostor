//! Signing in and out, and recognizing the caller of a request (AUTH-001, AUTH-003…AUTH-005,
//! AUTH-029). Sessions live in the database and are checked on every request, so ending one takes
//! effect at once (D-023).

use sqlx::PgConnection;
use taskmaster_domain::UserId;
use taskmaster_domain::accounts::PASSWORD_MAX;
use taskmaster_domain::auth::{
    Credential, Principal, Scopes, SessionCheck, SessionPolicy, TOUCH_INTERVAL, check_session,
};
use taskmaster_domain::validation::{Problem, Problems, Rejection};
use taskmaster_store::sessions::{self, Origin};
use taskmaster_store::users::{self, User};
use taskmaster_store::{PgPool, api_tokens};
use time::OffsetDateTime;

use crate::passwords::{Passwords, Verified};
use crate::secrets::{self, Secret, TOKEN_PREFIX};
use crate::{Error, Result};

/// Longest login people may type: an email address.
const LOGIN_MAX: usize = 254;

/// A password sign-in attempt.
#[derive(Clone)]
pub struct SignIn {
    /// Username or email.
    pub login: String,
    pub password: String,
    /// Keep the session for the remember-me duration instead of ending it when idle.
    pub remember: bool,
}

/// A started session.
#[derive(Debug, Clone)]
pub struct SignedIn {
    pub user: User,
    pub session: SessionInfo,
    /// The cookie value; only its hash is stored.
    pub secret: Secret,
}

/// What the owner of a session may know about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: i64,
    pub remember: bool,
    pub created_at: OffsetDateTime,
    /// A new password must be chosen before anything else (AUTH-010).
    pub must_change_password: bool,
}

/// Checks a login and password and starts a session (AUTH-001, AUTH-003).
pub async fn sign_in(
    pool: &PgPool,
    passwords: &Passwords,
    attempt: SignIn,
    policy: SessionPolicy,
    now: OffsetDateTime,
    origin: &Origin,
) -> Result<SignedIn> {
    let login = attempt.login.trim();
    let mut problems = Problems::new();
    if login.is_empty() {
        problems.add(
            "login",
            Rejection::new(Problem::Required, "login is required"),
        );
    }
    if attempt.password.is_empty() {
        problems.add(
            "password",
            Rejection::new(Problem::Required, "password is required"),
        );
    }
    problems.finish().map_err(Error::Invalid)?;
    // Longer values match no account; refuse them before hashing.
    if login.chars().count() > LOGIN_MAX || attempt.password.chars().count() > PASSWORD_MAX {
        return Err(Error::InvalidCredentials);
    }

    let mut conn = pool.acquire().await?;
    let found = users::find_by_login(&mut conn, login).await?;
    drop(conn);
    let stored = found
        .as_ref()
        .and_then(|found| found.password_hash.as_deref());
    // Always hash, even without an account, so the time taken tells nothing.
    let verified = passwords.verify(&attempt.password, stored).await?;
    let (Some(found), Verified::Match { rehash }) = (found, verified) else {
        return Err(Error::InvalidCredentials);
    };
    if found.user.disabled_at.is_some() {
        return Err(Error::InvalidCredentials);
    }
    if let (Some(new), Some(old)) = (rehash, found.password_hash.as_deref()) {
        users::upgrade_password_hash(pool, found.user.id, old, &new).await?;
    }

    let mut tx = pool.begin().await?;
    // Housekeeping: a user's ended sessions go when they sign in again.
    sessions::delete_expired_for_user(
        &mut tx,
        found.user.id,
        now - policy.idle,
        now - policy.remember,
    )
    .await?;
    let (mut session, secret) =
        start_session(&mut tx, found.user.id, attempt.remember, now, origin).await?;
    tx.commit().await?;
    session.must_change_password = found.must_change;
    Ok(SignedIn {
        user: found.user,
        session,
        secret,
    })
}

/// Creates a session inside the caller's transaction.
pub(crate) async fn start_session(
    conn: &mut PgConnection,
    user: UserId,
    remember: bool,
    now: OffsetDateTime,
    origin: &Origin,
) -> Result<(SessionInfo, Secret)> {
    let secret = secrets::session()?;
    let origin = Origin {
        ip: origin.ip.clone(),
        // Browsers send a few hundred characters; anything longer is not worth keeping.
        user_agent: origin
            .user_agent
            .as_ref()
            .map(|agent| agent.chars().take(512).collect()),
    };
    let id = sessions::insert(conn, user, &secret.hash, remember, now, &origin).await?;
    Ok((
        SessionInfo {
            id,
            remember,
            created_at: now,
            must_change_password: false,
        },
        secret,
    ))
}

/// The user behind a session cookie, if the session is valid at `now`. Records activity at most
/// once a minute; removes the session when it has expired.
pub async fn authenticate_session(
    pool: &PgPool,
    cookie: &str,
    policy: SessionPolicy,
    now: OffsetDateTime,
) -> Result<Option<Principal>> {
    let Some(session) = sessions::find_by_hash(pool, &secrets::hash(cookie)).await? else {
        return Ok(None);
    };
    if session.user_disabled {
        return Ok(None);
    }
    match check_session(
        policy,
        session.remember,
        session.created_at,
        session.last_seen_at,
        now,
    ) {
        SessionCheck::Expired => {
            sessions::delete(pool, session.id).await?;
            Ok(None)
        }
        SessionCheck::Active { touch } => {
            if touch {
                sessions::touch(pool, session.id, now).await?;
            }
            Ok(Some(Principal {
                user: session.user_id,
                credential: Credential::Session {
                    id: session.id,
                    must_change_password: session.must_change_password,
                },
            }))
        }
    }
}

/// The user behind a personal API token, if it exists and its owner may sign in. Records the use
/// at most once a minute.
pub async fn authenticate_token(
    pool: &PgPool,
    token: &str,
    now: OffsetDateTime,
) -> Result<Option<Principal>> {
    if !token.starts_with(TOKEN_PREFIX) {
        return Ok(None);
    }
    let Some(found) = api_tokens::find_by_hash(pool, &secrets::hash(token)).await? else {
        return Ok(None);
    };
    if found.user_disabled {
        return Ok(None);
    }
    if found
        .last_used_at
        .is_none_or(|used| now - used >= TOUCH_INTERVAL)
    {
        api_tokens::touch(pool, found.id, now).await?;
    }
    Ok(Some(Principal {
        user: found.user_id,
        credential: Credential::Token {
            id: found.id,
            scopes: Scopes::from_stored(&found.scopes),
        },
    }))
}

/// Which of these sessions and tokens may still be used at `now`, by the rules of sign-in
/// (without recording a use): the live gateway closes the sockets of the others.
pub async fn alive(
    pool: &PgPool,
    sessions_: &[i64],
    tokens: &[i64],
    policy: SessionPolicy,
    now: OffsetDateTime,
) -> Result<(Vec<i64>, Vec<i64>)> {
    let sessions_alive = if sessions_.is_empty() {
        Vec::new()
    } else {
        sessions::alive(pool, sessions_)
            .await?
            .into_iter()
            .filter(|s| {
                !s.user_disabled
                    && check_session(policy, s.remember, s.created_at, s.last_seen_at, now)
                        != SessionCheck::Expired
            })
            .map(|s| s.id)
            .collect()
    };
    let tokens_alive = if tokens.is_empty() {
        Vec::new()
    } else {
        api_tokens::alive(pool, tokens).await?
    };
    Ok((sessions_alive, tokens_alive))
}

/// The caller's own session.
pub async fn session(pool: &PgPool, id: i64) -> Result<SessionInfo> {
    let session = sessions::get(pool, id)
        .await?
        .ok_or(Error::NotFound("the session has ended"))?;
    Ok(SessionInfo {
        id: session.id,
        remember: session.remember,
        created_at: session.created_at,
        must_change_password: session.must_change_password,
    })
}

/// Ends the session of a cookie (AUTH-005). A cookie of a session that is already gone is fine.
pub async fn sign_out_by_cookie(pool: &PgPool, cookie: &str) -> Result<()> {
    sessions::delete_by_hash(pool, &secrets::hash(cookie)).await?;
    Ok(())
}
