//! Browser sessions (AUTH-003…AUTH-005). When a session expires is decided by
//! `trackflow_domain::auth::check_session`; this module only stores them.

use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use trackflow_domain::UserId;

/// A session found by its token, with what authentication needs to know about the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: i64,
    pub user_id: UserId,
    pub remember: bool,
    pub created_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    /// The user is disabled: the session no longer counts.
    pub user_disabled: bool,
    /// The user must choose a new password first (AUTH-010).
    pub must_change_password: bool,
}

/// Where a sign-in came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

pub async fn insert(
    conn: &mut PgConnection,
    user: UserId,
    token_hash: &[u8],
    remember: bool,
    now: OffsetDateTime,
    origin: &Origin,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into sessions (user_id, token_hash, remember, created_at, last_seen_at, ip,
                               user_agent)
         values ($1, $2, $3, $4, $4, $5::text::inet, $6)
         returning id",
        user,
        token_hash,
        remember,
        now,
        origin.ip.as_deref(),
        origin.user_agent.as_deref(),
    )
    .fetch_one(conn)
    .await
}

pub async fn find_by_hash(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<Session>, sqlx::Error> {
    sqlx::query_as!(
        Session,
        r#"select s.id, s.user_id, s.remember, s.created_at, s.last_seen_at,
                  (u.disabled_at is not null) as "user_disabled!",
                  coalesce(p.must_change, false) as "must_change_password!"
           from sessions s join users u on u.id = s.user_id
           left join passwords p on p.user_id = s.user_id
           where s.token_hash = $1"#,
        token_hash,
    )
    .fetch_optional(pool)
    .await
}

/// The sessions among `ids` that still exist, for the live gateway's sweep (the caller applies
/// the expiry rule).
pub async fn alive(pool: &PgPool, ids: &[i64]) -> Result<Vec<Session>, sqlx::Error> {
    sqlx::query_as!(
        Session,
        r#"select s.id as "id!", s.user_id as "user_id!", s.remember as "remember!",
                  s.created_at as "created_at!", s.last_seen_at as "last_seen_at!",
                  (u.disabled_at is not null) as "user_disabled!",
                  coalesce(p.must_change, false) as "must_change_password!"
           from sessions s join users u on u.id = s.user_id
           left join passwords p on p.user_id = s.user_id
           where s.id = any($1)"#,
        ids,
    )
    .fetch_all(pool)
    .await
}

pub async fn get(pool: &PgPool, id: i64) -> Result<Option<Session>, sqlx::Error> {
    sqlx::query_as!(
        Session,
        r#"select s.id, s.user_id, s.remember, s.created_at, s.last_seen_at,
                  (u.disabled_at is not null) as "user_disabled!",
                  coalesce(p.must_change, false) as "must_change_password!"
           from sessions s join users u on u.id = s.user_id
           left join passwords p on p.user_id = s.user_id
           where s.id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// Records activity.
pub async fn touch(pool: &PgPool, id: i64, now: OffsetDateTime) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update sessions set last_seen_at = $2 where id = $1 and last_seen_at < $2",
        id,
        now,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Ends one session; `false` when it was already gone.
pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
    let deleted = sqlx::query!("delete from sessions where id = $1", id)
        .execute(pool)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Ends the session with this token hash, if there is one.
pub async fn delete_by_hash(pool: &PgPool, token_hash: &[u8]) -> Result<bool, sqlx::Error> {
    let deleted = sqlx::query!("delete from sessions where token_hash = $1", token_hash)
        .execute(pool)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Ends every session of a user except `keep`. Returns how many ended.
pub async fn delete_for_user(
    conn: &mut PgConnection,
    user: UserId,
    keep: Option<i64>,
) -> Result<u64, sqlx::Error> {
    let deleted = match keep {
        Some(keep) => {
            sqlx::query!(
                "delete from sessions where user_id = $1 and id <> $2",
                user,
                keep
            )
            .execute(conn)
            .await?
        }
        None => {
            sqlx::query!("delete from sessions where user_id = $1", user)
                .execute(conn)
                .await?
        }
    };
    Ok(deleted.rows_affected())
}

/// Ends every session of these accounts (disabling, deleting). Returns how many ended.
pub async fn delete_for_users(
    conn: &mut PgConnection,
    users: &[UserId],
) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query!("delete from sessions where user_id = any($1)", users)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected())
}

/// Removes a user's sessions that ended: without "remember me" idle since `idle_before`, with it
/// created before `remembered_before`.
pub async fn delete_expired_for_user(
    conn: &mut PgConnection,
    user: UserId,
    idle_before: OffsetDateTime,
    remembered_before: OffsetDateTime,
) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query!(
        "delete from sessions
         where user_id = $1
           and ((not remember and last_seen_at <= $2) or (remember and created_at <= $3))",
        user,
        idle_before,
        remembered_before,
    )
    .execute(conn)
    .await?;
    Ok(deleted.rows_affected())
}

/// How many sessions a user has (for tests and the future sessions list).
pub async fn count_for_user(pool: &PgPool, user: UserId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(*) as "count!" from sessions where user_id = $1"#,
        user
    )
    .fetch_one(pool)
    .await
}
