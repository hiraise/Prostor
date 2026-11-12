//! Saved searches (QRY-063…QRY-068): a query under a name, shared like tags (QRY-065).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::accounts::UserRef;
use crate::search::QueryProblem;
use crate::sharing::{SharedWith, SharedWithInput, SharingAbilities};

/// A predefined search every person gets (QRY-068).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinSearchKey {
    AssignedToMe,
    CommentedByMe,
    ReportedByMe,
    AllIssues,
}

/// Who views (and so runs) and who edits a saved search.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SearchSharing {
    pub view: SharedWith,
    pub edit: SharedWith,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SearchSharingInput {
    #[serde(default)]
    pub view: SharedWithInput,
    #[serde(default)]
    pub edit: SharedWithInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SavedSearch {
    pub id: i64,
    pub name: String,
    pub owner: UserRef,
    /// The query with today's names of what it refers to, in the caller's words.
    pub query: String,
    /// `deleted_reference` for each part whose target is gone: such a search does not run.
    pub problems: Vec<QueryProblem>,
    #[schema(required = true)]
    pub builtin: Option<BuiltinSearchKey>,
    /// The project of a project's "unassigned" search (PRJ-019).
    #[schema(required = true)]
    pub project: Option<i64>,
    /// Only for the owner, editors and user administrators.
    #[schema(required = true)]
    pub sharing: Option<SearchSharing>,
    pub abilities: SharingAbilities,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NewSavedSearch {
    pub name: String,
    pub query: String,
}

/// Changes of a saved search; absent fields stay. Editors change the name and query; the owner or
/// a user administrator with share-views the sharing; a user administrator the owner.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct SavedSearchPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub sharing: Option<SearchSharingInput>,
    #[serde(default)]
    pub owner: Option<i64>,
    /// The version the change is based on.
    pub version: i64,
}
