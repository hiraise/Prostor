//! Sort keys (plan §3.5): at most three, then `id desc` as the tie-breaker; without `sort`,
//! `updated desc`.

use taskmaster_query::words::{self, Kind};
use taskmaster_query::{Direction, ProblemCode, SortKey, fold};

use super::resolve::R;
use super::{Binding, FieldRef, TextScope, TextTerm};
use crate::ValueId;
use crate::fields::FieldKind;

/// Sort keys a query may have before the tie-breaker.
pub const SORT_KEYS_MAX: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
    pub by: SortBy,
    pub direction: Direction,
}

/// What to sort by and how the store reads it. Empty values go last in both directions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortBy {
    Updated,
    Created,
    Resolved,
    Id,
    Votes,
    /// The count of visible comments.
    Comments,
    /// By project key.
    Project,
    /// By the reporter's full name.
    Reporter,
    Summary,
    /// Starred by the viewer first (ascending).
    Starred,
    /// A set field by the position of its values, set by set (D-024's union by position).
    Positions {
        field: FieldRef,
        positions: Vec<(ValueId, u32)>,
    },
    /// A user field by full name.
    Users {
        field: FieldRef,
    },
    /// A group field by name.
    Groups {
        field: FieldRef,
    },
    /// A number, duration, date or string field by value (`field.kind` picks the table).
    Scalar {
        field: FieldRef,
    },
    /// How well the issue matches the text terms (QRY-045); the default of a query with text.
    Relevance(Vec<(TextScope, TextTerm)>),
}

impl SortBy {
    /// `desc` unless the key reads naturally upwards (plan §3.5).
    pub fn default_direction(&self) -> Direction {
        match self {
            Self::Updated
            | Self::Created
            | Self::Resolved
            | Self::Votes
            | Self::Comments
            | Self::Relevance(_)
            | Self::Id => Direction::Desc,
            _ => Direction::Asc,
        }
    }
}

fn spec(by: SortBy, direction: Option<Direction>) -> SortSpec {
    let direction = direction.unwrap_or_else(|| by.default_direction());
    SortSpec { by, direction }
}

impl R<'_> {
    pub(super) fn sort(&mut self, keys: &[SortKey]) -> Vec<SortSpec> {
        let mut out = Vec::new();
        for (index, key) in keys.iter().enumerate() {
            if index >= SORT_KEYS_MAX {
                self.problem(ProblemCode::TooManySortKeys, key.name.span);
                continue;
            }
            if let Some(by) = self.sort_key(key) {
                out.push(spec(by, key.direction));
            }
        }
        if out.is_empty() {
            // A query with text ranks by relevance; OR keeps it (QRY-045).
            let by = if self.ranked.is_empty() {
                SortBy::Updated
            } else {
                SortBy::Relevance(self.ranked.clone())
            };
            out.push(spec(by, None));
        }
        if !out.iter().any(|s| s.by == SortBy::Id) {
            out.push(spec(SortBy::Id, None));
        }
        out
    }

    fn sort_key(&mut self, key: &SortKey) -> Option<SortBy> {
        let name = &key.name;
        let folded = fold(&name.text);
        if !name.quoted {
            if let Some(word) = words::keyword(&folded, &[Kind::Attribute]) {
                let by = match word.key {
                    "updated" => SortBy::Updated,
                    "created" => SortBy::Created,
                    "resolved_date" => SortBy::Resolved,
                    "id" => SortBy::Id,
                    "votes" => SortBy::Votes,
                    "comments" => SortBy::Comments,
                    "project" => SortBy::Project,
                    "reporter" => SortBy::Reporter,
                    "summary" => SortBy::Summary,
                    _ => {
                        self.problem(ProblemCode::NotSortable, name.span);
                        return None;
                    }
                };
                self.bindings
                    .push((name.span, Binding::Attribute(word.key.to_owned())));
                return Some(by);
            }
            if words::keyword(&folded, &[Kind::Value]).is_some_and(|w| w.key == "starred") {
                self.bindings
                    .push((name.span, Binding::Keyword("starred".to_owned())));
                return Some(SortBy::Starred);
            }
        }
        // Two fields with one name: the first in the catalog's order sorts.
        let Some(field) = self
            .cat
            .fields_named(&folded, &self.ctx.scope)
            .first()
            .copied()
        else {
            if self.cat.link_sides_named(&folded).is_empty() {
                self.problem(ProblemCode::UnknownAttribute, name.span);
            } else {
                self.problem(ProblemCode::NotSortable, name.span);
            }
            return None;
        };
        let fref = self.field_ref(field);
        let by = match field.kind {
            FieldKind::Text => {
                self.problem(ProblemCode::NotSortable, name.span);
                return None;
            }
            FieldKind::User => SortBy::Users { field: fref },
            FieldKind::Group => SortBy::Groups { field: fref },
            FieldKind::Enum
            | FieldKind::State
            | FieldKind::Version
            | FieldKind::Build
            | FieldKind::Owned => {
                let sets: Vec<_> = self
                    .cat
                    .attachments(field.id, &self.ctx.scope)
                    .iter()
                    .filter_map(|a| a.set)
                    .collect();
                let mut positions: Vec<(ValueId, u32)> = self
                    .cat
                    .values
                    .iter()
                    .filter(|v| sets.contains(&v.set))
                    .map(|v| (v.id, v.position))
                    .collect();
                positions.sort_unstable();
                positions.dedup();
                SortBy::Positions {
                    field: fref,
                    positions,
                }
            }
            _ => SortBy::Scalar { field: fref },
        };
        self.bindings.push((name.span, Binding::Field(field.id)));
        Some(by)
    }
}
