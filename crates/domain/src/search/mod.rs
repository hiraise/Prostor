//! Searching issues (QRY-*; plan `docs/plans/step-1.5.md` §3–§4): a parsed query
//! (`taskmaster_query`) and a [`Catalog`] of the names it mentions become a [`Filter`] and a sort
//! the store compiles to SQL.
//!
//! The service: parses the text, takes [`needs`], loads the projects, computes [`scope`], loads
//! the rest of the catalog for that scope, then calls [`resolve`].

pub mod catalog;
pub mod dates;
pub mod filter;
pub mod names;
mod resolve;
pub mod saved;
pub mod sort;
pub mod stored;
#[cfg(test)]
mod tests;
mod text;

use serde::{Deserialize, Serialize};
use taskmaster_query::{Problem, Span};

use crate::access::Projects;
use crate::links::Side;
use crate::{
    BoardId, FieldId, GroupId, IssueId, LinkTypeId, ProjectId, SprintId, TagId, UserId, ValueId,
};

pub use catalog::Catalog;
pub use dates::Clock;
pub use filter::{Bound, FieldRef, Filter, Range, Target, TextScope, TextTerm};
pub use names::{Needs, needs};
pub use resolve::{Scope, resolve, scope};
pub use sort::{SortBy, SortSpec};
pub use stored::Stored;
pub use text::TEXT_TERMS_MAX;

/// Who searches, when, and where.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub me: UserId,
    /// The request's time zone, else the instance's `default_time_zone`.
    pub clock: Clock,
    /// Projects where the viewer reads private fields (`issue.private.read` or
    /// `visibility.bypass`).
    pub private_readable: Projects,
    /// Where names are looked up: see [`scope`].
    pub scope: Vec<ProjectId>,
    /// The "hide resolved" toggle (QRY-054): adds "unresolved" unless the query is about
    /// resolution itself.
    pub hide_resolved: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub filter: Filter,
    /// The effective sort, ending with the tie-breaker `id desc`.
    pub sort: Vec<SortSpec>,
    pub notes: Vec<Problem>,
    /// The query names a state field, `resolved:` or `is: resolved/unresolved`.
    pub concerns_resolution: bool,
    /// The query is one issue ID (QRY-016): the UI opens the issue.
    pub single_id: Option<IssueId>,
    /// What each name and value resolved to, for the stored form (§3.6, 1.5d). A name meaning
    /// two fields has two entries with one span.
    pub bindings: Vec<(Span, Binding)>,
}

/// What one atom of a query resolved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "t", content = "v")]
pub enum Binding {
    Field(FieldId),
    /// A key of `taskmaster_query::words`.
    Attribute(String),
    Keyword(String),
    Value(ValueId),
    User(UserId),
    Group(GroupId),
    Project(ProjectId),
    Tag(TagId),
    LinkSide(LinkTypeId, Side),
    Issue(IssueId),
    /// `board: X`; with a [`Binding::Sprint`] on the same span, the board of `sprint: "X/S"`.
    Board(BoardId),
    Sprint(SprintId),
    /// `sprint: current` (`None`) or `sprint: "X/current"`: resolved to IDs on each run.
    CurrentSprint(Option<BoardId>),
    /// Numbers, dates, strings, empty labels: kept as written.
    Literal(String),
}

impl Serialize for Side {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for Side {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let key = String::deserialize(d)?;
        Side::parse(&key).ok_or_else(|| serde::de::Error::custom("unknown link side"))
    }
}

pub(crate) fn contains(projects: &Projects, id: ProjectId) -> bool {
    match projects {
        Projects::All => true,
        Projects::Only(ids) => ids.contains(&id),
    }
}
