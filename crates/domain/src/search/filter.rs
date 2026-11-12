//! The resolved tree the store compiles to SQL (plan §4): every name and value is an ID, every date
//! an instant or a calendar date.

use time::{Date, OffsetDateTime};

use crate::fields::FieldKind;
use crate::links::Side;
use crate::{BoardId, FieldId, IssueId, LinkTypeId, ProjectId, SprintId, TagId, UserId};

/// One end of a range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bound<T> {
    Open,
    Included(T),
    Excluded(T),
}

/// A range of values; both ends may be open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range<T> {
    pub low: Bound<T>,
    pub high: Bound<T>,
}

impl<T> Range<T> {
    /// `[low, high]`.
    pub fn closed(low: T, high: T) -> Self {
        Self {
            low: Bound::Included(low),
            high: Bound::Included(high),
        }
    }

    /// `[low, high)`; `None` is open.
    pub fn half_open(low: Option<T>, high: Option<T>) -> Self {
        Self {
            low: low.map_or(Bound::Open, Bound::Included),
            high: high.map_or(Bound::Open, Bound::Excluded),
        }
    }
}

/// A field as a condition sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRef {
    pub id: FieldId,
    /// Picks the value table.
    pub kind: FieldKind,
    /// Whether an issue may hold several values of the field (sorting and filters that name the
    /// field itself may then not skip positions: a later value can match where the smallest does
    /// not).
    pub multi: bool,
    /// Projects where the field is private and the viewer may not read private fields (FLD-062):
    /// there the condition is false, so `i.project_id <> all(hidden)` joins the atom. Usually empty.
    pub hidden: Vec<ProjectId>,
}

/// Where a text term is looked for (QRY-036).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextScope {
    /// Summary, description, the text fields readable here and visible comments.
    All {
        fields: Vec<FieldRef>,
    },
    Summary,
    Description,
    /// Visible comments.
    Comments,
    /// One text field.
    Field(FieldRef),
}

/// One text term as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextTerm {
    /// A bare word: any form of it (stems).
    Word(String),
    /// A quoted single word: that form only (QRY-037).
    Exact(String),
    /// Quoted words in this order.
    Phrase(String),
}

impl TextTerm {
    pub fn text(&self) -> &str {
        match self {
            Self::Word(t) | Self::Exact(t) | Self::Phrase(t) => t,
        }
    }
}

/// The other issue of a link condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Any readable issue.
    Any,
    Issues(Vec<IssueId>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    True,
    False,
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),
    /// The issue holds one of these values (value IDs, or account / group IDs for user and group
    /// fields). `implied`: every value is resolved (`Some(true)`) or unresolved (`Some(false)`)
    /// in a project's only state field, so the store adds the matching `resolved_at` condition
    /// (D-024).
    RefValues {
        field: FieldRef,
        ids: Vec<i64>,
        implied: Option<bool>,
    },
    /// The field is empty in an issue of one of `projects` (those that have it attached).
    FieldEmpty {
        field: FieldRef,
        projects: Vec<ProjectId>,
    },
    FieldAny {
        field: FieldRef,
    },
    /// Integers and durations (minutes).
    Int {
        field: FieldRef,
        range: Range<i64>,
    },
    Float {
        field: FieldRef,
        range: Range<f64>,
    },
    Date {
        field: FieldRef,
        range: Range<Date>,
    },
    DateTime {
        field: FieldRef,
        range: Range<OffsetDateTime>,
    },
    /// The exact text, case-insensitive.
    Str {
        field: FieldRef,
        value: String,
    },
    /// `resolved_at is not null` / `is null`.
    Resolved(bool),
    Projects(Vec<ProjectId>),
    Issues(Vec<IssueId>),
    Reporter(Vec<UserId>),
    Updater(Vec<UserId>),
    /// Wrote a comment the viewer can see.
    Commenter(Vec<UserId>),
    /// Reporter, any of these user fields, or commenter.
    Involves {
        users: Vec<UserId>,
        fields: Vec<FieldRef>,
    },
    Created(Range<OffsetDateTime>),
    Updated(Range<OffsetDateTime>),
    ResolvedAt(Range<OffsetDateTime>),
    /// Has a visible comment written within the range.
    Commented(Range<OffsetDateTime>),
    /// Has a visible comment.
    Comments,
    /// Has a visible attachment.
    Attachments,
    /// The description is empty.
    DescriptionEmpty,
    Tags(Vec<TagId>),
    /// No tag the viewer can see.
    NoTags,
    /// The viewer stars the issue.
    Starred,
    /// Linked to the target; `sides: None` means by any type.
    Linked {
        sides: Option<Vec<(LinkTypeId, Side)>>,
        target: Target,
    },
    Votes(Range<i64>),
    Text {
        scope: TextScope,
        term: TextTerm,
    },
    /// The description or a visible comment mentions one of these people (CNT-058).
    MentionsUsers(Vec<UserId>),
    /// The description or a visible comment mentions this issue by any of its IDs.
    MentionsIssue(IssueId),
    /// Mentioned by this issue's description or its visible comments.
    MentionedIn(IssueId),
    /// In a sprint of one of these boards (QRY-049).
    OnBoards(Vec<BoardId>),
    /// In one of these sprints.
    Sprints(Vec<SprintId>),
}

impl Filter {
    /// AND that drops `True`, flattens nested ANDs and collapses on `False`.
    pub fn and(parts: Vec<Filter>) -> Filter {
        let mut out = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                Filter::True => {}
                Filter::False => return Filter::False,
                Filter::And(inner) => out.extend(inner),
                other => out.push(other),
            }
        }
        match out.len() {
            0 => Filter::True,
            1 => out.pop().unwrap_or(Filter::True),
            _ => Filter::And(out),
        }
    }

    /// OR that drops `False`, flattens nested ORs and collapses on `True`.
    pub fn or(parts: Vec<Filter>) -> Filter {
        let mut out = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                Filter::False => {}
                Filter::True => return Filter::True,
                Filter::Or(inner) => out.extend(inner),
                other => out.push(other),
            }
        }
        match out.len() {
            0 => Filter::False,
            1 => out.pop().unwrap_or(Filter::False),
            _ => Filter::Or(out),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn not(inner: Filter) -> Filter {
        match inner {
            Filter::True => Filter::False,
            Filter::False => Filter::True,
            Filter::Not(inner) => *inner,
            other => Filter::Not(Box::new(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_simplify() {
        assert_eq!(Filter::and(vec![]), Filter::True);
        assert_eq!(Filter::or(vec![]), Filter::False);
        assert_eq!(
            Filter::and(vec![Filter::True, Filter::Starred]),
            Filter::Starred
        );
        assert_eq!(
            Filter::and(vec![Filter::Starred, Filter::False]),
            Filter::False
        );
        assert_eq!(
            Filter::or(vec![Filter::Comments, Filter::True]),
            Filter::True
        );
        assert_eq!(
            Filter::and(vec![
                Filter::and(vec![Filter::Starred, Filter::Comments]),
                Filter::Attachments
            ]),
            Filter::And(vec![Filter::Starred, Filter::Comments, Filter::Attachments])
        );
        assert_eq!(Filter::not(Filter::not(Filter::Starred)), Filter::Starred);
    }
}
