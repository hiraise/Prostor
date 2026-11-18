//! Staged uploads (CNT-017, CNT-019, CNT-034): a file is sent once, kept for its sender, and then
//! attached to an issue or comment, or made an avatar or logo. Unused uploads go a day later.
//!
//! The body streams into the file store outside any transaction; only then is the row written,
//! so a slow upload holds no database connection. The kind comes from the bytes (D-033).

use bytes::Bytes;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use taskmaster_domain::access::Access;
use taskmaster_domain::files::{self as rules, STAGED_HOURS, STAGED_MAX};
use taskmaster_domain::{UploadId, UserId};
use taskmaster_files::{FileStore, ReceiveError};
use taskmaster_store::jobs::{self, Enqueue, JobError, JobKind};
use taskmaster_store::uploads::{self as store, NewUpload, Upload};
use taskmaster_store::{PgPool, users};
use time::{Duration, OffsetDateTime};

use crate::{Error, Result};

const TOO_MANY: Error =
    Error::Conflict("too many files are waiting to be attached; attach or remove some");

/// Whether the account may upload: enabled, not deleted, not the guest.
async fn require_uploader(conn: &mut sqlx::PgConnection, user: UserId) -> Result<()> {
    let account = users::get(&mut *conn, user)
        .await?
        .ok_or(Error::NotFound("the account no longer exists"))?;
    if account.is_guest() || account.is_deleted() || account.disabled_at.is_some() {
        return Err(Error::Forbidden(
            taskmaster_domain::access::permission::PermissionSet::EMPTY,
        ));
    }
    Ok(())
}

/// Receives a file for the actor (CNT-017): any enabled account but the guest, at most
/// [`STAGED_MAX`] waiting, at most `max` bytes (CNT-019).
pub async fn receive<S, E>(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    name: &str,
    body: S,
    max: u64,
) -> Result<Upload>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let name = rules::name(name).map_err(|r| Error::rejected("name", r))?;
    {
        let mut conn = pool.acquire().await?;
        require_uploader(&mut conn, actor.user()).await?;
        if store::count_of_owner(&mut conn, actor.user()).await? >= STAGED_MAX {
            return Err(TOO_MANY);
        }
    }
    let received = match files.receive(body, max).await {
        Ok(received) => received,
        Err(ReceiveError::TooLarge(limit)) => return Err(Error::TooLarge(limit)),
        Err(ReceiveError::Body(problem)) => {
            return Err(Error::field(
                "body",
                taskmaster_domain::validation::Problem::Invalid,
                problem,
            ));
        }
        Err(ReceiveError::Io(error)) => {
            return Err(Error::Internal(format!("cannot store the file: {error}")));
        }
    };
    let kind = rules::sniff(&received.head);
    let stored = async {
        let mut tx = pool.begin().await?;
        if store::count_of_owner(&mut tx, actor.user()).await? >= STAGED_MAX {
            return Err(TOO_MANY);
        }
        let upload = store::insert(
            &mut tx,
            &NewUpload {
                owner: actor.user(),
                storage_key: &received.key,
                name: &name,
                size: i64::try_from(received.size).unwrap_or(i64::MAX),
                content_type: kind.content_type(),
                sha256: &received.sha256,
            },
        )
        .await?;
        jobs::enqueue(
            &mut tx,
            &UploadExpiry { upload: upload.id },
            Enqueue {
                run_at: Some(OffsetDateTime::now_utc() + Duration::hours(STAGED_HOURS)),
                ..Enqueue::default()
            },
        )
        .await?;
        tx.commit().await?;
        Ok(upload)
    }
    .await;
    if stored.is_err() {
        let _ = files.remove(&[received.key]).await;
    }
    stored
}

/// A person's own staged upload.
pub async fn get(pool: &PgPool, actor: &Access, id: UploadId) -> Result<Upload> {
    store::get(pool, id)
        .await?
        .filter(|u| u.owner_id == actor.user())
        .ok_or(Error::NotFound("no such upload"))
}

/// Throws away one's own staged upload.
pub async fn discard(pool: &PgPool, actor: &Access, id: UploadId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let found = store::lock_of_owner(&mut tx, actor.user(), &[id]).await?;
    if found.is_empty() {
        return Err(Error::NotFound("no such upload"));
    }
    let keys = store::delete(&mut tx, &[id]).await?;
    crate::files::queue_removal(&mut tx, keys).await?;
    tx.commit().await?;
    Ok(())
}

/// Removes an upload nobody attached (`uploads.expire`), a day after it came. Nothing when it was
/// used meanwhile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadExpiry {
    pub upload: UploadId,
}

impl JobKind for UploadExpiry {
    const KIND: &'static str = "uploads.expire";
    const CANCELLABLE: bool = false;
    const MAX_ATTEMPTS: i32 = 10;
}

pub async fn run_expiry(pool: &PgPool, job: UploadExpiry) -> Result<(), JobError> {
    let mut tx = pool.begin().await?;
    let keys = store::delete(&mut tx, &[job.upload]).await?;
    crate::files::queue_removal(&mut tx, keys)
        .await
        .map_err(|e| JobError::Retry(e.to_string()))?;
    tx.commit().await?;
    Ok(())
}
