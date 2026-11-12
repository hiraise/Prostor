//! Links between issues and link types (LNK): what the link endpoints take and answer.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::projects::ProjectRef;

/// How a link type joins issues (LNK-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkDirection {
    /// One name on both sides.
    Undirected,
    /// A name for each side.
    Directed,
    /// A name for each side; each target has one source and there are no loops (subtasks).
    Hierarchy,
}

/// A link type every instance starts with (LNK-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinLinkType {
    Related,
    Dependency,
    Duplicate,
    Subtask,
}

/// Which end of a link an issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkSide {
    /// The parent of a subtask, the original of a duplicate, the issue that blocks.
    Source,
    Target,
}

/// A link type as a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkTypeRef {
    pub id: i64,
    /// What administrators call it.
    pub name: String,
    /// How the source refers to the target.
    pub source_name: String,
    /// How the target refers to the source; `null` for undirected types.
    #[schema(required = true)]
    pub target_name: Option<String>,
    pub direction: LinkDirection,
    #[schema(required = true)]
    pub builtin: Option<BuiltinLinkType>,
}

/// A link type in full (LNK-018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkType {
    pub id: i64,
    pub name: String,
    pub source_name: String,
    #[schema(required = true)]
    pub target_name: Option<String>,
    pub direction: LinkDirection,
    #[schema(required = true)]
    pub builtin: Option<BuiltinLinkType>,
    /// Built in and relied on: cannot be changed or deleted.
    pub locked: bool,
    /// In the trash: its links are hidden until it is restored.
    #[schema(required = true)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub deleted_at: Option<OffsetDateTime>,
    /// How many links it has; `null` without `system.read`.
    #[schema(required = true)]
    pub usage: Option<i64>,
    pub version: i64,
}

/// All link types, oldest first; deleted ones only for holders of `system.read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkTypeList {
    pub items: Vec<LinkType>,
}

/// A new link type (LNK-019).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewLinkType {
    pub name: String,
    pub source_name: String,
    /// Required unless `direction` is `undirected`, which ignores it.
    #[serde(default)]
    pub target_name: Option<String>,
    pub direction: LinkDirection,
}

/// Changes of a link type (LNK-020); omitted members stay. Renames apply to every link at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkTypePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub source_name: Option<String>,
    /// Needed when an undirected type becomes directed.
    #[serde(default)]
    pub target_name: Option<String>,
    /// Becoming `undirected` drops the target name; becoming `hierarchy` is refused while links
    /// of the type give a target several sources or form a loop.
    #[serde(default)]
    pub direction: Option<LinkDirection>,
    /// The version the change started from.
    pub version: i64,
}

/// What deleting a link type hides (LNK-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkTypeImpact {
    pub links: i64,
}

/// An issue at the other end of a link, or a parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueBrief {
    pub id: i64,
    pub readable_id: String,
    pub summary: String,
    pub project: ProjectRef,
    pub resolved: bool,
    /// Limited to an audience (ISS-041).
    pub restricted: bool,
}

/// A link as one issue's panel shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkedIssue {
    /// The link's ID, for removing it.
    pub link: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub linked_at: OffsetDateTime,
    pub issue: IssueBrief,
}

/// The links of one name on an issue: one type seen from one side (LNK-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkGroup {
    pub link_type: LinkTypeRef,
    /// The issue's side; `source` for undirected types.
    pub side: LinkSide,
    /// The name of this side, such as "parent of".
    pub name: String,
    pub total: i64,
    pub unresolved: i64,
    /// Unresolved issues first, then the newest links; the first ten in a panel.
    pub items: Vec<LinkedIssue>,
    /// Pass to the group's endpoint for the next rows; `null` when there are none.
    #[schema(required = true)]
    pub next_cursor: Option<String>,
}

/// An issue's links the caller may see (LNK-004, LNK-010): groups in the order of the link types,
/// the source side first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueLinks {
    pub groups: Vec<LinkGroup>,
    /// The caller may add and remove links here.
    pub can_link: bool,
}

/// Links to add from an issue (LNK-005).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AddLinks {
    pub link_type: i64,
    /// The side of this issue: `source` makes it the parent of the others with the subtask type.
    pub side: LinkSide,
    /// Internal IDs of the issues to link, 1–100.
    pub issues: Vec<i64>,
    /// Without notifications; needs `issue.silent`.
    #[serde(default)]
    pub silent: bool,
}

/// What adding links did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinksAdded {
    /// Issues linked now.
    pub added: Vec<i64>,
    /// Issues that had a link of this type with the issue already.
    pub existing: Vec<i64>,
    /// The issue's links afterwards.
    pub links: IssueLinks,
    /// What the project's rules said (AUT-023, step 1.9).
    pub messages: Vec<crate::rules::Message>,
}

/// What removing a link did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinksRemoved {
    /// What the project's rules said (AUT-023, step 1.9).
    pub messages: Vec<crate::rules::Message>,
}

/// Removing a link (LNK-008).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RemoveLinkQuery {
    /// Without notifications; needs `issue.silent`.
    pub silent: Option<bool>,
}

/// A link made together with a new issue (ISS-009, LNK-006, LNK-007).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewIssueLink {
    pub link_type: i64,
    /// The new issue's side: `target` makes it a subtask of `issue` with the subtask type.
    pub side: LinkSide,
    pub issue: i64,
}

/// An issue's subtasks the caller may see (LNK-014).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SubtaskCount {
    pub total: i64,
    pub unresolved: i64,
}

/// A link named by history (HIST-005): its type with the current names, the side of the issue
/// the record belongs to, and the issue at the other end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LoggedLink {
    pub link_type: LinkTypeRef,
    pub side: LinkSide,
    /// The name of that side, such as "child of".
    pub name: String,
    pub issue: IssueBrief,
}
