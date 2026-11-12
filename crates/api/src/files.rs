//! Files (D-033): staged uploads, attachments of issues and comments, avatars and project logos.
//!
//! A file is sent once to `POST /uploads` as the raw request body and comes back as an `Upload`;
//! its ID is then given where the file goes: an issue's attachments, a new issue or comment, an
//! avatar or a logo. Unused uploads are removed a day later.
//!
//! Attachment URLs are signed for the caller and work without a session until the end of the
//! third UTC day after the answer (CNT-026); they are the same all day, so browsers cache them.
//! They stop working when the file is deleted or the caller can no longer see it.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::accounts::UserRef;
use crate::issues::{Audience, AudienceInput};

/// How a file is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// PNG, JPEG, GIF or WebP: shown as a picture.
    Image,
    /// MP4, WebM or Ogg: played in the page.
    Video,
    /// Opens in the browser's viewer.
    Pdf,
    /// Anything else downloads.
    Other,
}

/// A file sent and not attached yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Upload {
    pub id: i64,
    pub name: String,
    /// Bytes.
    pub size: i64,
    /// What the server recognized from the file's bytes.
    pub content_type: String,
    pub kind: FileKind,
    /// The file, for its sender only (with their session or token).
    pub url: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UploadQuery {
    /// The file's name, 1–255 characters without `/` and `\`.
    pub name: String,
}

/// What the caller may do with an attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AttachmentAbilities {
    /// Rename it, change its audience.
    pub update: bool,
    pub delete: bool,
}

/// A file of an issue (CNT-017) or of a comment on it (CNT-018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Attachment {
    pub id: i64,
    /// The issue's internal ID.
    pub issue: i64,
    /// The comment it was posted with.
    #[schema(required = true)]
    pub comment: Option<i64>,
    /// Unique on the issue.
    pub name: String,
    /// Bytes.
    pub size: i64,
    pub content_type: String,
    pub kind: FileKind,
    /// Pictures' size in pixels, once known (shortly after the upload).
    #[schema(required = true)]
    pub width: Option<i32>,
    #[schema(required = true)]
    pub height: Option<i32>,
    /// The file, signed for the caller.
    pub url: String,
    /// A smaller copy of a large picture (inside 480 × 480); `null`: use `url`.
    #[schema(required = true)]
    pub thumbnail_url: Option<String>,
    pub author: UserRef,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Whom it is limited to, inside the issue's (or comment's) audience; both empty: the same
    /// people.
    pub audience: Audience,
    pub can: AttachmentAbilities,
    pub version: i64,
}

/// Files to attach (CNT-017).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NewAttachments {
    /// Staged uploads of the caller, in the order to attach.
    pub uploads: Vec<i64>,
    /// Without notifications (needs `issue.silent`).
    #[serde(default)]
    pub silent: bool,
}

/// Changes of an attachment; absent fields stay.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct AttachmentPatch {
    #[serde(default)]
    pub name: Option<String>,
    /// Empty lists: the same people as the issue.
    #[serde(default)]
    pub audience: Option<AudienceInput>,
    /// The version the change started from.
    pub version: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AttachmentQuery {
    /// The last ID of the previous page.
    pub cursor: Option<i64>,
    /// 1–100; default 100.
    pub limit: Option<u32>,
}

/// A page of an issue's files, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AttachmentPage {
    pub attachments: Vec<Attachment>,
    /// Pass as `cursor` for the next page; `null` at the end.
    #[schema(required = true)]
    pub next: Option<i64>,
}

/// What a signed link carries (CNT-026).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FileLinkQuery {
    /// Whom the link was made for.
    pub u: i64,
    /// Until when it works, Unix seconds.
    pub e: i64,
    /// The signature.
    pub s: String,
    /// The thumbnail rather than the file.
    #[serde(default)]
    pub thumb: Option<u8>,
}

/// A file a text names (`tf-file:` markers): an attachment of the issue the caller sees, or for
/// a preview one of the caller's uploads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FileRef {
    /// The name as the text writes it.
    pub name: String,
    pub kind: FileKind,
    pub size: i64,
    #[schema(required = true)]
    pub width: Option<i32>,
    #[schema(required = true)]
    pub height: Option<i32>,
    pub url: String,
    #[schema(required = true)]
    pub thumbnail_url: Option<String>,
}

/// A staged upload to make a picture of.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct PictureInput {
    pub upload: i64,
}

/// An avatar or logo now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Picture {
    /// `null`: the generated one.
    #[schema(required = true)]
    pub url: Option<String>,
}
