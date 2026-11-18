//! The snapshot of names the service loads for one query ([`super::names::needs`] says which), and
//! the lookups the resolver runs on it. Loading more than needed is harmless.

use std::collections::BTreeSet;

use taskmaster_query::{fold, fold_value};

use super::filter::Filter;
use crate::boards::sprints::SprintDates;
use crate::fields::FieldKind;
use crate::links::Side;
use crate::{
    BoardId, FieldId, GroupId, IssueId, LinkTypeId, ProjectId, SetId, SprintId, TagId, UserId,
    ValueId,
};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    /// Projects the viewer can read.
    pub projects: Vec<ProjectEntry>,
    /// Fields attached to projects in scope.
    pub fields: Vec<FieldEntry>,
    /// Attachments of those fields to projects in scope.
    pub attached: Vec<Attached>,
    /// Values of the sets in `attached` whose folded name is wanted, and every value of the sets
    /// a range, a comparison, a sort or `resolved`/`unresolved` needs whole.
    pub values: Vec<ValueEntry>,
    /// Accounts (not deleted) whose login or full name is wanted.
    pub users: Vec<UserEntry>,
    /// Groups whose name is wanted, with their members.
    pub groups: Vec<GroupEntry>,
    /// Tags the viewer may see whose name is wanted.
    pub tags: Vec<TagEntry>,
    /// Sides of link types that are not deleted.
    pub link_sides: Vec<LinkSideEntry>,
    /// Readable issues by the IDs the query names.
    pub issues: Vec<IssueEntry>,
    /// Boards the viewer can view (QRY-049): those whose name is wanted, or all of them when
    /// [`super::Needs::all_boards`]. A board in query mode counts only when named: `board: Q`
    /// means «matches Q's query»; `any`, `none` and sprints count boards of cards.
    pub boards: Vec<BoardEntry>,
    /// The visible sprints (not internal) of `boards`, archived ones included.
    pub sprints: Vec<SprintEntry>,
    /// The text terms of [`super::Needs::texts`] that hold only common words (their text
    /// query is empty, QRY-041).
    pub common_words: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    pub id: ProjectId,
    pub key: String,
    pub former_keys: Vec<String>,
    pub name: String,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldEntry {
    pub id: FieldId,
    pub name: String,
    pub aliases: Vec<String>,
    pub kind: FieldKind,
    pub multi: bool,
    /// The built-in key, such as `spent_time`; `None` for a custom field.
    pub builtin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    pub project: ProjectId,
    pub field: FieldId,
    pub set: Option<SetId>,
    /// FLD-057.
    pub can_be_empty: bool,
    /// FLD-056; empty when not set.
    pub empty_label: String,
    pub private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueEntry {
    pub id: ValueId,
    pub set: SetId,
    pub name: String,
    /// The index in the set ordered by rank.
    pub position: u32,
    /// A state value flagged resolved.
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEntry {
    pub id: UserId,
    pub login: String,
    pub full_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupEntry {
    pub id: GroupId,
    pub name: String,
    pub members: Vec<UserId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagEntry {
    pub id: TagId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardEntry {
    pub id: BoardId,
    pub name: String,
    /// `None` for a board of cards; for a board in query mode, the issues its query matches
    /// within its projects (`False` when the query does not resolve for the viewer).
    pub query: Option<Filter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SprintEntry {
    pub board: BoardId,
    pub name: String,
    /// `dates.id` is the sprint's ID.
    pub dates: SprintDates,
}

impl SprintEntry {
    pub fn id(&self) -> SprintId {
        self.dates.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSideEntry {
    pub link_type: LinkTypeId,
    pub side: Side,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueEntry {
    /// As [`super::names::issue_key`] writes it: `WEB-42` uppercase, or the internal ID in digits.
    pub text: String,
    pub id: IssueId,
}

/// Who wrote a value: a user field resolves to people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum People {
    One(UserId),
    /// Several accounts share the full name.
    Ambiguous,
    None,
}

impl Catalog {
    pub fn project(&self, id: ProjectId) -> Option<&ProjectEntry> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// Projects by current or former key (any case) or by name (folded).
    pub fn projects_named(&self, text: &str) -> Vec<ProjectId> {
        projects_named(&self.projects, text)
    }

    /// Fields whose name or alias folds to `folded`, attached to a project in `scope`.
    pub fn fields_named(&self, folded: &str, scope: &[ProjectId]) -> Vec<&FieldEntry> {
        self.fields
            .iter()
            .filter(|f| fold(&f.name) == folded || f.aliases.iter().any(|a| fold(a) == folded))
            .filter(|f| !self.attachments(f.id, scope).is_empty())
            .collect()
    }

    pub fn attachments(&self, field: FieldId, scope: &[ProjectId]) -> Vec<&Attached> {
        self.attached
            .iter()
            .filter(|a| a.field == field && scope.contains(&a.project))
            .collect()
    }

    /// Values of `set` whose name matches the value text.
    pub fn values_named(&self, set: SetId, text: &str) -> Vec<&ValueEntry> {
        let folded = fold_value(text);
        self.values
            .iter()
            .filter(|v| v.set == set && fold_value(&v.name) == folded)
            .collect()
    }

    pub fn set_values(&self, set: SetId) -> impl Iterator<Item = &ValueEntry> {
        self.values.iter().filter(move |v| v.set == set)
    }

    /// An account by login (case-insensitive), else by full name (folded).
    pub fn user(&self, text: &str) -> People {
        let lower = fold_value(text);
        if let Some(user) = self.users.iter().find(|u| fold_value(&u.login) == lower) {
            return People::One(user.id);
        }
        let folded = fold(text);
        let mut named = self.users.iter().filter(|u| fold(&u.full_name) == folded);
        match (named.next(), named.next()) {
            (Some(user), None) => People::One(user.id),
            (Some(_), Some(_)) => People::Ambiguous,
            _ => People::None,
        }
    }

    pub fn group(&self, text: &str) -> Option<&GroupEntry> {
        let folded = fold_value(text);
        self.groups.iter().find(|g| fold_value(&g.name) == folded)
    }

    /// Every visible tag of that name: one's own and others' shared ones may share a name.
    pub fn tags_named(&self, text: &str) -> Vec<TagId> {
        let folded = fold_value(text);
        self.tags
            .iter()
            .filter(|t| fold_value(&t.name) == folded)
            .map(|t| t.id)
            .collect()
    }

    pub fn link_sides_named(&self, folded: &str) -> Vec<(LinkTypeId, Side)> {
        self.link_sides
            .iter()
            .filter(|s| fold(&s.name) == folded)
            .map(|s| (s.link_type, s.side))
            .collect()
    }

    /// Boards by folded name: board names are not unique.
    pub fn boards_named(&self, text: &str) -> Vec<BoardId> {
        let folded = fold_value(text);
        self.boards
            .iter()
            .filter(|b| fold_value(&b.name) == folded)
            .map(|b| b.id)
            .collect()
    }

    /// Boards of cards (not in query mode) by folded name.
    pub fn card_boards_named(&self, text: &str) -> Vec<BoardId> {
        let folded = fold_value(text);
        self.boards
            .iter()
            .filter(|b| b.query.is_none() && fold_value(&b.name) == folded)
            .map(|b| b.id)
            .collect()
    }

    /// Sprints by folded name, on `board` or on any board.
    pub fn sprints_named(&self, text: &str, board: Option<BoardId>) -> Vec<SprintId> {
        let folded = fold_value(text);
        self.sprints
            .iter()
            .filter(|s| board.is_none_or(|b| s.board == b) && fold_value(&s.name) == folded)
            .map(SprintEntry::id)
            .collect()
    }

    /// The current sprint of `board` on `today` (BRD-099).
    pub fn current_sprint(&self, board: BoardId, today: time::Date) -> Option<SprintId> {
        let dates: Vec<SprintDates> = self
            .sprints
            .iter()
            .filter(|s| s.board == board)
            .map(|s| s.dates.clone())
            .collect();
        crate::boards::sprints::current(&dates, today)
    }

    pub fn issue(&self, text: &str) -> Option<IssueId> {
        let key = super::names::issue_key(text)?;
        self.issues.iter().find(|i| i.text == key).map(|i| i.id)
    }
}

/// Projects by current or former key (any case), else by name (folded).
pub fn projects_named(projects: &[ProjectEntry], text: &str) -> Vec<ProjectId> {
    let upper = text.trim().to_uppercase();
    let by_key: Vec<ProjectId> = projects
        .iter()
        .filter(|p| p.key == upper || p.former_keys.contains(&upper))
        .map(|p| p.id)
        .collect();
    if !by_key.is_empty() {
        return by_key;
    }
    let folded = fold(text);
    projects
        .iter()
        .filter(|p| fold(&p.name) == folded)
        .map(|p| p.id)
        .collect()
}
