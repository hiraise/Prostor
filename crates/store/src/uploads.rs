//! Staged uploads (CNT-017, CNT-034): files a person sent and has not attached yet. A job removes
//! each a day later; attaching moves it into `attachments`.

use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::{UploadId, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    pub id: UploadId,
    pub owner_id: UserId,
    pub storage_key: String,
    pub name: String,
    pub size: i64,
    pub content_type: String,
    pub sha256: Vec<u8>,
    pub created_at: OffsetDateTime,
}

/// An upload to record. Values are already validated.
#[derive(Debug, Clone)]
pub struct NewUpload<'a> {
    pub owner: UserId,
    pub storage_key: &'a str,
    pub name: &'a str,
    pub size: i64,
    pub content_type: &'a str,
    pub sha256: &'a [u8],
}

pub async fn insert(conn: &mut PgConnection, new: &NewUpload<'_>) -> Result<Upload, sqlx::Error> {
    sqlx::query_as!(
        Upload,
        "insert into uploads (owner_id, storage_key, name, size, content_type, sha256)
         values ($1, $2, $3, $4, $5, $6)
         returning id, owner_id, storage_key, name, size, content_type, sha256, created_at",
        new.owner,
        new.storage_key,
        new.name,
        new.size,
        new.content_type,
        new.sha256
    )
    .fetch_one(conn)
    .await
}

/// How many uploads a person keeps staged; locks their account row so two uploads at once
/// cannot both pass the limit.
pub async fn count_of_owner(conn: &mut PgConnection, owner: UserId) -> Result<i64, sqlx::Error> {
    sqlx::query!("select id from users where id = $1 for update", owner)
        .fetch_optional(&mut *conn)
        .await?;
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from uploads where owner_id = $1"#,
        owner
    )
    .fetch_one(conn)
    .await
}

pub async fn get(db: impl PgExecutor<'_>, id: UploadId) -> Result<Option<Upload>, sqlx::Error> {
    sqlx::query_as!(
        Upload,
        "select id, owner_id, storage_key, name, size, content_type, sha256, created_at
         from uploads where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

/// A person's uploads among `ids`, locked, in the order of `ids`; others' are left out.
pub async fn lock_of_owner(
    conn: &mut PgConnection,
    owner: UserId,
    ids: &[UploadId],
) -> Result<Vec<Upload>, sqlx::Error> {
    let found = sqlx::query_as!(
        Upload,
        "select id, owner_id, storage_key, name, size, content_type, sha256, created_at
         from uploads where owner_id = $1 and id = any($2) order by id for update",
        owner,
        ids
    )
    .fetch_all(conn)
    .await?;
    Ok(ids
        .iter()
        .filter_map(|id| found.iter().find(|u| u.id == *id).cloned())
        .collect())
}

/// A person's uploads among `ids`, unlocked: previews of texts that embed them.
pub async fn of_owner(
    db: impl PgExecutor<'_>,
    owner: UserId,
    ids: &[UploadId],
) -> Result<Vec<Upload>, sqlx::Error> {
    sqlx::query_as!(
        Upload,
        "select id, owner_id, storage_key, name, size, content_type, sha256, created_at
         from uploads where owner_id = $1 and id = any($2) order by id",
        owner,
        ids
    )
    .fetch_all(db)
    .await
}

/// Removes upload rows; answers the storage keys of those that were there.
pub async fn delete(conn: &mut PgConnection, ids: &[UploadId]) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "delete from uploads where id = any($1) returning storage_key",
        ids
    )
    .fetch_all(conn)
    .await
}

/// Removes the rows of uploads whose file now belongs to an attachment.
pub async fn forget(conn: &mut PgConnection, ids: &[UploadId]) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from uploads where id = any($1)", ids)
        .execute(conn)
        .await?;
    Ok(())
}

/// Removes every staged upload of accounts being deleted; answers their storage keys.
pub async fn delete_of_owners(
    conn: &mut PgConnection,
    owners: &[UserId],
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "delete from uploads where owner_id = any($1) returning storage_key",
        owners
    )
    .fetch_all(conn)
    .await
}
