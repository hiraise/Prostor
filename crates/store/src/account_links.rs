//! Emailed one-time links (AUTH-008, AUTH-014, ACC-004): password resets, email verifications and
//! invitations. Only the SHA-256 of a link's secret is stored; using a link deletes it.

use prostor_domain::UserId;
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub id: i64,
    pub user_id: UserId,
    /// `password_reset`, `email_verification` or `invitation`
    /// (`prostor_domain::auth::LinkPurpose`).
    pub purpose: String,
    pub email: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

/// Stores a new link, replacing older links of the same purpose for the account.
pub async fn replace(
    conn: &mut PgConnection,
    user: UserId,
    purpose: &str,
    email: &str,
    token_hash: &[u8],
    created_at: OffsetDateTime,
    expires_at: OffsetDateTime,
) -> Result<i64, sqlx::Error> {
    sqlx::query!(
        "delete from account_links where user_id = $1 and purpose = $2",
        user,
        purpose,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query_scalar!(
        "insert into account_links (user_id, purpose, email, token_hash, created_at, expires_at)
         values ($1, $2, $3, $4, $5, $6) returning id",
        user,
        purpose,
        email,
        token_hash,
        created_at,
        expires_at,
    )
    .fetch_one(conn)
    .await
}

/// The link with this secret hash, locked so it can be used once.
pub async fn lock_by_hash(
    conn: &mut PgConnection,
    token_hash: &[u8],
) -> Result<Option<Link>, sqlx::Error> {
    sqlx::query_as!(
        Link,
        "select id, user_id, purpose, email, created_at, expires_at
         from account_links where token_hash = $1 for update",
        token_hash,
    )
    .fetch_optional(conn)
    .await
}

/// The link with this secret hash, for showing what a link is for before it is used.
pub async fn find_by_hash(pool: &PgPool, token_hash: &[u8]) -> Result<Option<Link>, sqlx::Error> {
    sqlx::query_as!(
        Link,
        "select id, user_id, purpose, email, created_at, expires_at
         from account_links where token_hash = $1",
        token_hash,
    )
    .fetch_optional(pool)
    .await
}

/// When the newest link of this purpose for the account was made.
pub async fn latest(
    conn: &mut PgConnection,
    user: UserId,
    purpose: &str,
) -> Result<Option<OffsetDateTime>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select max(created_at) as "at" from account_links
           where user_id = $1 and purpose = $2"#,
        user,
        purpose,
    )
    .fetch_one(conn)
    .await
}

/// Deletes links of the account; `purpose` `None` deletes all of them.
pub async fn delete_for_user(
    conn: &mut PgConnection,
    user: UserId,
    purpose: Option<&str>,
) -> Result<(), sqlx::Error> {
    match purpose {
        Some(purpose) => {
            sqlx::query!(
                "delete from account_links where user_id = $1 and purpose = $2",
                user,
                purpose,
            )
            .execute(conn)
            .await?;
        }
        None => {
            sqlx::query!("delete from account_links where user_id = $1", user)
                .execute(conn)
                .await?;
        }
    }
    Ok(())
}

/// Deletes the links of these accounts (account deletion).
pub async fn delete_for_users(
    conn: &mut PgConnection,
    users: &[UserId],
) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from account_links where user_id = any($1)", users)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete(conn: &mut PgConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from account_links where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Housekeeping: removes links that expired before `now`.
pub async fn delete_expired(
    conn: &mut PgConnection,
    now: OffsetDateTime,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!("delete from account_links where expires_at < $1", now)
        .execute(conn)
        .await?;
    Ok(done.rows_affected())
}
