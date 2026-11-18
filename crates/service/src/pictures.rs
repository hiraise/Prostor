//! Avatars (ACC-017) and project logos (PRJ-007): pictures made from a staged upload, or for
//! avatars from Gravatar. Design: decisions.md D-033.
//!
//! Gravatar (owner's decision, 2026-10-22): the server downloads the picture once, when the person
//! asks, and keeps it as their uploaded avatar. Browsers never contact Gravatar, and nobody sees
//! an email hash.
//!
//! The new picture is written first; the transaction then points the row at it and queues the
//! old picture's removal, so a failure never leaves a row without its file.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Limited};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use prostor_domain::access::Access;
use prostor_domain::access::permission::Permission;
use prostor_domain::files::Kind;
use prostor_domain::validation::Problem;
use prostor_domain::{ProjectId, UploadId, UserId};
use prostor_files::FileStore;
use prostor_store::uploads::{self, Upload};
use prostor_store::{PgPool, projects, users};
use sha2::{Digest, Sha256};

use crate::images::{self, Encoded, Unusable};
use crate::people::{Names, Shown};
use crate::{Error, Result};

/// Largest upload made into a picture.
const SOURCE_MAX_BYTES: i64 = 64 * 1024 * 1024;
/// How long the whole Gravatar request may take.
const GRAVATAR_TIMEOUT: Duration = Duration::from_secs(5);
/// Largest picture taken from Gravatar.
const GRAVATAR_MAX_BYTES: usize = 1024 * 1024;

/// The Gravatar import (ACC-017): where to ask, and the HTTP client.
#[derive(Clone)]
pub struct Gravatar {
    base: url::Url,
    client: Client<HttpsConnector<HttpConnector>, Empty<Bytes>>,
    timeout: Duration,
}

impl std::fmt::Debug for Gravatar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gravatar")
            .field("base", &self.base.as_str())
            .finish()
    }
}

impl Gravatar {
    /// A client for the service at `base` (`https://gravatar.com/avatar/`), over rustls with the
    /// ring provider the database and mail connections use.
    pub fn new(base: &str) -> Result<Self> {
        let mut base = url::Url::parse(base)
            .map_err(|e| Error::Internal(format!("the Gravatar address is not a URL: {e}")))?;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_provider_and_webpki_roots(provider)
            .map_err(|e| Error::Internal(format!("TLS for Gravatar: {e}")))?
            .https_or_http()
            .enable_http1()
            .build();
        Ok(Self {
            base,
            client: Client::builder(TokioExecutor::new()).build(connector),
            timeout: GRAVATAR_TIMEOUT,
        })
    }

    /// How long a request may take (tests shorten it).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The picture for an address: SHA-256 of the trimmed, lowercased address; 256 pixels; a
    /// 404 when there is none.
    async fn fetch(&self, email: &str) -> Result<Vec<u8>> {
        let hash: String = Sha256::digest(email.trim().to_lowercase().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let mut url = self
            .base
            .join(&hash)
            .map_err(|e| Error::GravatarUnavailable(e.to_string()))?;
        url.set_query(Some("s=256&d=404"));
        let request = http::Request::get(url.as_str())
            .header(http::header::USER_AGENT, "prostor")
            .body(Empty::<Bytes>::new())
            .map_err(|e| Error::GravatarUnavailable(e.to_string()))?;
        let work = async {
            let response = self
                .client
                .request(request)
                .await
                .map_err(|e| Error::GravatarUnavailable(e.to_string()))?;
            if response.status() == http::StatusCode::NOT_FOUND {
                return Err(Error::GravatarNotFound);
            }
            if !response.status().is_success() {
                return Err(Error::GravatarUnavailable(format!(
                    "answered {}",
                    response.status()
                )));
            }
            let body = Limited::new(response.into_body(), GRAVATAR_MAX_BYTES)
                .collect()
                .await
                .map_err(|e| Error::GravatarUnavailable(e.to_string()))?;
            Ok(body.to_bytes().to_vec())
        };
        tokio::time::timeout(self.timeout, work)
            .await
            .map_err(|_| Error::GravatarUnavailable("no answer in time".to_owned()))?
    }
}

/// Reads one's own staged upload for a picture.
async fn source(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    id: UploadId,
) -> Result<(Upload, Vec<u8>, Kind)> {
    let upload = uploads::get(pool, id)
        .await?
        .filter(|u| u.owner_id == actor.user())
        .ok_or_else(|| Error::field("upload", Problem::Invalid, "no such upload"))?;
    let kind = Kind::of_content_type(&upload.content_type);
    if !kind.is_image() {
        return Err(Unusable::NotAPicture.error("upload"));
    }
    if upload.size > SOURCE_MAX_BYTES {
        return Err(Unusable::TooLarge.error("upload"));
    }
    let bytes = files
        .read(&upload.storage_key)
        .await
        .map_err(|e| Error::Internal(format!("cannot read the upload: {e}")))?;
    Ok((upload, bytes, kind))
}

async fn make(
    bytes: Vec<u8>,
    kind: Kind,
    work: fn(&[u8], Kind) -> Result<Encoded, Unusable>,
    path: &'static str,
) -> Result<Encoded> {
    images::off_thread(move || work(&bytes, kind))
        .await?
        .map_err(|u| u.error(path))
}

// Avatars (ACC-017).

/// Changing a person's avatar: one's own with `user.self`, others' with `user.update`.
fn may_change_avatar(actor: &Access, user: UserId) -> Result<()> {
    actor.require(if actor.user() == user {
        Permission::UserSelf
    } else {
        Permission::UserUpdate
    })?;
    Ok(())
}

/// Points the account at a new avatar (or none) and queues the old picture and a used upload
/// for removal. The new picture is removed again when this fails.
async fn store_avatar(
    pool: &PgPool,
    files: &FileStore,
    user: UserId,
    picture: Option<Encoded>,
    used: Option<UploadId>,
) -> Result<Option<String>> {
    let key = match &picture {
        Some(p) => Some(
            files
                .put(&p.bytes, p.kind.extension())
                .await
                .map_err(|e| Error::Internal(format!("cannot store the picture: {e}")))?,
        ),
        None => None,
    };
    let done = async {
        let mut tx = pool.begin().await?;
        users::lock(&mut tx, user)
            .await?
            .filter(|u| !u.is_deleted())
            .ok_or(Error::NotFound("no such account"))?;
        let mut old: Vec<String> = users::set_avatar_key(&mut tx, user, key.as_deref())
            .await?
            .into_iter()
            .collect();
        if let Some(upload) = used {
            old.extend(uploads::delete(&mut tx, &[upload]).await?);
        }
        crate::files::queue_removal(&mut tx, old).await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    if let Err(error) = done {
        if let Some(key) = key {
            let _ = files.remove(&[key]).await;
        }
        return Err(error);
    }
    Ok(key)
}

/// Makes a staged upload someone's avatar: the middle square of the picture (ACC-017). Answers
/// the new picture's key.
pub async fn set_avatar(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    user: UserId,
    upload: UploadId,
) -> Result<Option<String>> {
    may_change_avatar(actor, user)?;
    let (upload, bytes, kind) = source(pool, files, actor, upload).await?;
    let picture = make(bytes, kind, images::avatar, "upload").await?;
    store_avatar(pool, files, user, Some(picture), Some(upload.id)).await
}

/// Goes back to the generated avatar.
pub async fn reset_avatar(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    user: UserId,
) -> Result<()> {
    may_change_avatar(actor, user)?;
    store_avatar(pool, files, user, None, None).await?;
    Ok(())
}

/// Takes the person's Gravatar picture as their avatar, once (ACC-017, owner's decision).
pub async fn import_gravatar(
    pool: &PgPool,
    files: &FileStore,
    gravatar: &Gravatar,
    actor: &Access,
    user: UserId,
) -> Result<Option<String>> {
    may_change_avatar(actor, user)?;
    let account = users::get(pool, user)
        .await?
        .filter(|u| !u.is_deleted())
        .ok_or(Error::NotFound("no such account"))?;
    let email = account.email.ok_or(Error::GravatarNotFound)?;
    let bytes = gravatar.fetch(&email).await?;
    let kind = prostor_domain::files::sniff(&bytes);
    if !kind.is_image() {
        return Err(Error::GravatarUnavailable(
            "the answer is not a picture".to_owned(),
        ));
    }
    let picture = images::off_thread(move || images::avatar(&bytes, kind))
        .await?
        .map_err(|_| Error::GravatarUnavailable("the picture cannot be read".to_owned()))?;
    store_avatar(pool, files, user, Some(picture), None).await
}

/// The last part of a key: what URLs name, so a new picture gets a new URL.
pub fn file_of(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

/// The avatar file to send: the person is shown to the viewer by name and has this picture now.
pub async fn avatar_file(
    pool: &PgPool,
    names: &Names,
    viewer: &Access,
    user: UserId,
    file: &str,
) -> Result<String> {
    let account = users::get(pool, user)
        .await?
        .ok_or(Error::NotFound("no such picture"))?;
    if names.show(viewer, &account).shown != Shown::Named {
        return Err(Error::NotFound("no such picture"));
    }
    account
        .avatar_key
        .filter(|key| file_of(key) == file)
        .ok_or(Error::NotFound("no such picture"))
}

// Project logos (PRJ-007).

async fn store_logo(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    project: ProjectId,
    picture: Option<Encoded>,
    used: Option<UploadId>,
) -> Result<Option<String>> {
    let key = match &picture {
        Some(p) => Some(
            files
                .put(&p.bytes, p.kind.extension())
                .await
                .map_err(|e| Error::Internal(format!("cannot store the picture: {e}")))?,
        ),
        None => None,
    };
    let done = async {
        let mut tx = pool.begin().await?;
        let found = crate::projects::lock_visible(&mut tx, actor, project).await?;
        crate::projects::require(actor, Permission::ProjectUpdate, found.id)?;
        crate::projects::require_active(&found)?;
        let mut old: Vec<String> = projects::set_logo_key(&mut tx, project, key.as_deref())
            .await?
            .into_iter()
            .collect();
        if let Some(upload) = used {
            old.extend(uploads::delete(&mut tx, &[upload]).await?);
        }
        crate::files::queue_removal(&mut tx, old).await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    if let Err(error) = done {
        if let Some(key) = key {
            let _ = files.remove(&[key]).await;
        }
        return Err(error);
    }
    Ok(key)
}

/// Makes a staged upload the project's logo: the whole picture inside a square (PRJ-007). Needs
/// `project.update` in an active project.
pub async fn set_logo(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    project: ProjectId,
    upload: UploadId,
) -> Result<Option<String>> {
    {
        // Refuse before decoding anything.
        let mut conn = pool.acquire().await?;
        let found = crate::projects::visible(&mut conn, actor, project).await?;
        crate::projects::require(actor, Permission::ProjectUpdate, found.id)?;
        crate::projects::require_active(&found)?;
    }
    let (upload, bytes, kind) = source(pool, files, actor, upload).await?;
    let picture = make(bytes, kind, images::logo, "upload").await?;
    store_logo(pool, files, actor, project, Some(picture), Some(upload.id)).await
}

/// Goes back to the generated logo.
pub async fn reset_logo(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    project: ProjectId,
) -> Result<()> {
    store_logo(pool, files, actor, project, None, None).await?;
    Ok(())
}

/// The logo file to send: the project is visible and has this picture now.
pub async fn logo_file(
    pool: &PgPool,
    viewer: &Access,
    project: ProjectId,
    file: &str,
) -> Result<String> {
    let mut conn = pool.acquire().await?;
    let found = crate::projects::visible(&mut conn, viewer, project)
        .await
        .map_err(|_| Error::NotFound("no such picture"))?;
    found
        .logo_key
        .filter(|key| file_of(key) == file)
        .ok_or(Error::NotFound("no such picture"))
}
