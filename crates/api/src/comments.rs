//! Comments on issues (CNT-001…CNT-005): what the comment endpoints take and answer.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::accounts::UserRef;
use crate::content::TextRefs;
use crate::files::Attachment;
use crate::issues::{Audience, AudienceInput};

/// What the caller may do with a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommentAbilities {
    pub update: bool,
    pub delete: bool,
    /// Bring a deleted comment back.
    pub restore: bool,
}

/// When and by whom a comment was deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommentDeletion {
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    #[schema(required = true)]
    pub by: Option<UserRef>,
}

/// A comment (CNT-001). A deleted one is a placeholder (CNT-005): author, time and `deleted`;
/// its text, HTML and audience are empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Comment {
    pub id: i64,
    /// The issue's internal ID.
    pub issue: i64,
    pub author: UserRef,
    /// Markdown.
    pub text: String,
    /// The text rendered; see `TextRefs` for its markers.
    pub html: String,
    pub refs: TextRefs,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// `null` until the first edit.
    #[schema(required = true)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    #[schema(required = true)]
    pub updater: Option<UserRef>,
    /// Whom it is limited to, inside the issue's audience; both empty: the same people as the
    /// issue (CNT-003).
    pub audience: Audience,
    /// The files posted with it that the caller sees (CNT-018); empty when deleted.
    pub files: Vec<Attachment>,
    #[schema(required = true)]
    pub deleted: Option<CommentDeletion>,
    pub can: CommentAbilities,
    pub version: i64,
}

/// The comment after posting, with what the project's rules said (AUT-023, step 1.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommentAnswer {
    pub comment: Comment,
    pub messages: Vec<crate::rules::Message>,
}

/// A new comment (CNT-001).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NewComment {
    /// Markdown, at most 64 KiB.
    pub text: String,
    /// Omit for the same people as the issue.
    #[serde(default)]
    pub audience: Option<AudienceInput>,
    /// Without notifications (needs `issue.silent`).
    #[serde(default)]
    pub silent: bool,
    /// The caller's staged uploads to post with it (CNT-018); then `text` may be empty.
    #[serde(default)]
    pub uploads: Vec<i64>,
}

/// Changes of a comment (CNT-004); absent fields stay.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct CommentPatch {
    #[serde(default)]
    pub text: Option<String>,
    /// Empty lists: the same people as the issue.
    #[serde(default)]
    pub audience: Option<AudienceInput>,
    /// The version the edit started from.
    pub version: i64,
    #[serde(default)]
    pub silent: bool,
    /// The caller's staged uploads to add (CNT-018).
    #[serde(default)]
    pub add_uploads: Vec<i64>,
}
