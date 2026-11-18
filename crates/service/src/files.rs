//! File links and file removal (D-033, CNT-026).
//!
//! Answers that show files carry links signed for the viewer: the attachment, the viewer and an
//! expiry, keyed by the instance secret over the attachment's and the viewer's secrets. Opening a
//! link needs no session: the signature names the person, and their current access decides, so
//! a link never shows more than they may see now. Anything wrong answers "not found".
//!
//! Files leave the file store only through the `files.remove` job, queued in the transaction that
//! drops their rows, so a rolled-back change never loses a file.

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::files::{self as rules, Kind, Link};
use taskmaster_domain::{AttachmentId, UserId};
use taskmaster_files::FileStore;
use taskmaster_store::attachments::{self, Attachment};
use taskmaster_store::jobs::{self, Enqueue, JobError, JobKind};
use taskmaster_store::{PgPool, comments, instance, issues, users};
use time::OffsetDateTime;

use crate::secrets::random_bytes;
use crate::{Error, Result};

/// Signs file links with the instance secret.
#[derive(Clone)]
pub struct Signer {
    secret: Arc<[u8]>,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signer(***)")
    }
}

/// A link's query part, for one viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedLink {
    pub user: UserId,
    /// Unix seconds.
    pub expires: i64,
    pub signature: String,
    /// The thumbnail's signature, when the file has one.
    pub thumbnail_signature: Option<String>,
}

impl Signer {
    /// Loads the instance secret, creating it on the first start.
    pub async fn load(pool: &PgPool) -> Result<Self> {
        let generated = random_bytes::<32>()?;
        let secret = instance::secret(pool, &generated).await?;
        Ok(Self::with_secret(&secret))
    }

    pub fn with_secret(secret: &[u8]) -> Self {
        Self {
            secret: Arc::from(secret),
        }
    }

    /// A link to a file for a viewer, valid until `expires`.
    pub fn link(
        &self,
        viewer: UserId,
        viewer_secret: &[u8],
        file: &Attachment,
        expires: OffsetDateTime,
    ) -> SignedLink {
        let link = Link {
            attachment: file.id,
            attachment_secret: &file.secret,
            user: viewer,
            user_secret: viewer_secret,
            expires: expires.unix_timestamp(),
            thumbnail: false,
        };
        let signature = URL_SAFE_NO_PAD.encode(rules::sign(&self.secret, &link));
        let thumbnail_signature = file.thumbnail_key.as_ref().map(|_| {
            URL_SAFE_NO_PAD.encode(rules::sign(
                &self.secret,
                &Link {
                    thumbnail: true,
                    ..link
                },
            ))
        });
        SignedLink {
            user: viewer,
            expires: link.expires,
            signature,
            thumbnail_signature,
        }
    }

    fn verify(&self, link: &Link<'_>, signature: &str) -> bool {
        URL_SAFE_NO_PAD
            .decode(signature)
            .is_ok_and(|bytes| rules::verify(&self.secret, link, &bytes))
    }
}

/// The viewer's link secret, made on first use.
pub(crate) async fn viewer_secret(conn: &mut PgConnection, user: UserId) -> Result<Vec<u8>> {
    if let Some((_, secret)) = users::link_secrets(&mut *conn, &[user]).await?.pop() {
        return Ok(secret);
    }
    let generated = random_bytes::<32>()?;
    Ok(users::link_secret(&mut *conn, user, &generated).await?)
}

/// A new attachment's own secret.
pub(crate) fn new_secret() -> Result<Vec<u8>> {
    Ok(random_bytes::<32>()?.to_vec())
}

/// What a link says, as it came in the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkQuery {
    pub user: UserId,
    pub expires: i64,
    pub signature: String,
    pub thumbnail: bool,
}

/// A file to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    /// Its key in the file store.
    pub key: String,
    pub kind: Kind,
    /// The name it downloads under.
    pub name: String,
    pub expires: OffsetDateTime,
}

const NO_FILE: Error = Error::NotFound("no such file");

/// Checks a link (CNT-026): a file that is not deleted, the signature, the expiry. Answers the
/// file and whose access to check next.
pub async fn check_link(
    pool: &PgPool,
    signer: &Signer,
    id: AttachmentId,
    query: &LinkQuery,
    now: OffsetDateTime,
) -> Result<(Attachment, UserId)> {
    let expires = OffsetDateTime::from_unix_timestamp(query.expires).map_err(|_| NO_FILE)?;
    if expires <= now {
        return Err(NO_FILE);
    }
    let mut conn = pool.acquire().await?;
    let file = attachments::get(&mut *conn, id)
        .await?
        .filter(|f| !f.is_deleted())
        .ok_or(NO_FILE)?;
    let (_, user_secret) = users::link_secrets(&mut *conn, &[query.user])
        .await?
        .pop()
        .ok_or(NO_FILE)?;
    let link = Link {
        attachment: file.id,
        attachment_secret: &file.secret,
        user: query.user,
        user_secret: &user_secret,
        expires: query.expires,
        thumbnail: query.thumbnail,
    };
    if !signer.verify(&link, &query.signature) {
        return Err(NO_FILE);
    }
    Ok((file, query.user))
}

/// Opens a file for the person a link was made for, by their access now (CNT-026, ACC-049): the
/// issue, the comment of a comment's file, and the file's own audience.
pub async fn open(
    pool: &PgPool,
    viewer: &Access,
    file: Attachment,
    thumbnail: bool,
    expires: OffsetDateTime,
) -> Result<Served> {
    let mut conn = pool.acquire().await?;
    let found = issues::get(&mut *conn, file.issue_id).await?;
    let (issue, _) = crate::issues::checked(&mut conn, viewer, found)
        .await
        .map_err(|_| NO_FILE)?;
    if let Some(comment) = file.comment_id {
        let comment = comments::get(&mut *conn, comment).await?.ok_or(NO_FILE)?;
        if comment.is_deleted() || !crate::comments::can_see(viewer, &issue, &comment) {
            return Err(NO_FILE);
        }
    }
    if !crate::attachments::can_see(viewer, &issue, &file) {
        return Err(NO_FILE);
    }
    let (key, kind) = if thumbnail {
        let key = file.thumbnail_key.clone().ok_or(NO_FILE)?;
        let kind = if key.ends_with(".png") {
            Kind::Png
        } else {
            Kind::Jpeg
        };
        (key, kind)
    } else {
        (
            file.storage_key.clone(),
            Kind::of_content_type(&file.content_type),
        )
    };
    Ok(Served {
        key,
        kind,
        name: file.name,
        expires,
    })
}

/// Removes files from the store (`files.remove`). Idempotent: missing files are fine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesRemoval {
    pub keys: Vec<String>,
}

impl JobKind for FilesRemoval {
    const KIND: &'static str = "files.remove";
    const CANCELLABLE: bool = false;
    const MAX_ATTEMPTS: i32 = 10;
}

/// Queues the removal of files in the caller's transaction; nothing for no keys.
pub(crate) async fn queue_removal(conn: &mut PgConnection, keys: Vec<String>) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }
    jobs::enqueue(conn, &FilesRemoval { keys }, Enqueue::default()).await?;
    Ok(())
}

pub async fn run_removal(store: &FileStore, job: FilesRemoval) -> Result<(), JobError> {
    store
        .remove(&job.keys)
        .await
        .map_err(|e| JobError::Retry(format!("cannot remove files: {e}")))
}
