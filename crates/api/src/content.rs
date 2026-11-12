//! Rich text (CNT-038, D-031): previews and what rendered texts refer to.
//!
//! Rendered HTML is sanitized on the server and carries markers that are ordinary links with our
//! schemes: `tf-user:<account ID>` for mentions, `tf-issue:<ID as written>` for issue IDs,
//! `tf-file:<name>` for files of the issue. Each answer with rendered text has `refs` saying what
//! the markers stand for, for the caller; a marker without a ref is shown as its text.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::accounts::UserRef;
use crate::files::FileRef;

/// What a text may contain (CNT-039).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkdownProfile {
    /// Issue descriptions, text fields and comments: mentions, issue IDs and files.
    #[default]
    Full,
    /// Project descriptions: Markdown alone.
    Basic,
}

/// A text to preview.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct MarkdownPreview {
    /// Markdown, at most 256 KiB.
    pub text: String,
    #[serde(default)]
    pub profile: MarkdownProfile,
    /// The issue the text belongs to; must be visible to the caller. Its files show where the
    /// text names them.
    #[serde(default)]
    pub issue: Option<i64>,
    /// The caller's staged uploads the text may name (CNT-034).
    #[serde(default)]
    pub uploads: Vec<i64>,
}

/// An issue ID in a text that names an issue the caller may see (CNT-056).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueMention {
    /// As written, which may be a former ID.
    pub token: String,
    pub id: i64,
    /// The issue's current ID.
    pub readable_id: String,
    pub summary: String,
    pub resolved: bool,
}

/// What the markers of rendered texts stand for, for the caller.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TextRefs {
    /// People mentioned (`tf-user:` markers).
    pub users: Vec<UserRef>,
    /// Issue IDs that resolve to issues the caller may see (`tf-issue:` markers).
    pub issues: Vec<IssueMention>,
    /// Files the caller sees (`tf-file:` markers); a name without one is shown as missing.
    pub files: Vec<FileRef>,
}

/// A rendered text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Rendered {
    /// Sanitized HTML with markers.
    pub html: String,
    pub refs: TextRefs,
}
