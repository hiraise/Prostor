//! Personal API tokens (AUTH-029, AUTH-030). Scope keys: `taskmaster_domain::auth::Scope`.

use sqlx::{PgConnection, PgPool};
use taskmaster_domain::UserId;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiToken {
    pub id: i64,
    pub user_id: UserId,
    pub name: String,
    pub scopes: Vec<String>,
    pub created_by: Option<UserId>,
    pub created_at: OffsetDateTime,
    pub last_used_at: Option<OffsetDateTime>,
    pub version: i64,
}

/// A token found by its secret, with what authentication needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenAuth {
    pub id: i64,
    pub user_id: UserId,
    pub scopes: Vec<String>,
    pub last_used_at: Option<OffsetDateTime>,
    /// The owner is disabled: the token no longer works.
    pub user_disabled: bool,
}

pub struct NewToken<'a> {
    pub user: UserId,
    pub name: &'a str,
    pub scopes: &'a [String],
    pub token_hash: &'a [u8],
    pub created_by: UserId,
}

pub async fn insert(
    conn: &mut PgConnection,
    token: &NewToken<'_>,
) -> Result<ApiToken, sqlx::Error> {
    sqlx::query_as!(
        ApiToken,
        "insert into api_tokens (user_id, name, scopes, token_hash, created_by)
         values ($1, $2, $3, $4, $5)
         returning id, user_id, name, scopes, created_by, created_at, last_used_at, version",
        token.user,
        token.name,
        token.scopes,
        token.token_hash,
        token.created_by,
    )
    .fetch_one(conn)
    .await
}

/// A page of a user's tokens, newest first, after the token `before` (exclusive).
pub async fn list_for_user(
    pool: &PgPool,
    user: UserId,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<ApiToken>, sqlx::Error> {
    match before {
        Some(before) => {
            sqlx::query_as!(
                ApiToken,
                "select id, user_id, name, scopes, created_by, created_at, last_used_at, version
                 from api_tokens where user_id = $1 and id < $2
                 order by id desc limit $3",
                user,
                before,
                limit,
            )
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as!(
                ApiToken,
                "select id, user_id, name, scopes, created_by, created_at, last_used_at, version
                 from api_tokens where user_id = $1
                 order by id desc limit $2",
                user,
                limit,
            )
            .fetch_all(pool)
            .await
        }
    }
}

pub async fn find_by_hash(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<TokenAuth>, sqlx::Error> {
    sqlx::query_as!(
        TokenAuth,
        r#"select t.id, t.user_id, t.scopes, t.last_used_at,
                  (u.disabled_at is not null) as "user_disabled!"
           from api_tokens t join users u on u.id = t.user_id
           where t.token_hash = $1"#,
        token_hash,
    )
    .fetch_optional(pool)
    .await
}

/// The tokens among `ids` that still exist and whose owner may sign in.
pub async fn alive(pool: &PgPool, ids: &[i64]) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "select t.id from api_tokens t join users u on u.id = t.user_id
         where t.id = any($1) and u.disabled_at is null",
        ids,
    )
    .fetch_all(pool)
    .await
}

/// Records a use.
pub async fn touch(pool: &PgPool, id: i64, now: OffsetDateTime) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update api_tokens set last_used_at = $2
         where id = $1 and (last_used_at is null or last_used_at < $2)",
        id,
        now,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Deletes every token of these accounts (account deletion). Returns how many.
pub async fn delete_for_users(
    conn: &mut PgConnection,
    users: &[UserId],
) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query!("delete from api_tokens where user_id = any($1)", users)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected())
}

/// Deletes a token of this user; returns it, or `None` when the user has no such token.
pub async fn delete(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
) -> Result<Option<ApiToken>, sqlx::Error> {
    sqlx::query_as!(
        ApiToken,
        "delete from api_tokens where id = $1 and user_id = $2
         returning id, user_id, name, scopes, created_by, created_at, last_used_at, version",
        id,
        user,
    )
    .fetch_optional(conn)
    .await
}
