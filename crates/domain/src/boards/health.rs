//! Board health (BRD-023; plan 1.8 §3.16): what in the settings keeps the board from working
//! (errors) or from working fully (warnings), computed with the board from what it holds.

use crate::fields::FieldKind;
use crate::{FieldId, ProjectId, ValueId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// The board shows no cards.
    Error,
    /// Part of the board does not work.
    Warning,
}

/// A field a board setting names besides the column field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    Lane,
    Color,
    Estimation,
    OriginalEstimation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Finding {
    /// Every board project was deleted.
    NoProjects,
    NoColumnField,
    NoColumns,
    /// A board project does not have the column field: its issues are not on the board.
    ColumnFieldMissing {
        project: ProjectId,
    },
    /// A column's value is not in the set the project uses for the column field.
    ColumnValueMissing {
        project: ProjectId,
        value: ValueId,
    },
    /// An archived board project (PRJ-062): its issues show but cannot change.
    ArchivedProject {
        project: ProjectId,
    },
    /// The field of a setting is in no board project.
    FieldMissing {
        slot: Slot,
    },
    /// The field of a setting is of a kind the setting does not take.
    FieldWrongKind {
        slot: Slot,
    },
    /// The viewer does not view the backlog search.
    BacklogHidden,
    /// A query board's project gives new issues a column value that is no column: they do not
    /// show (BRD-028).
    DefaultNotColumn {
        project: ProjectId,
    },
}

impl Finding {
    pub fn severity(self) -> Severity {
        match self {
            Finding::NoProjects
            | Finding::NoColumnField
            | Finding::NoColumns
            | Finding::ColumnFieldMissing { .. } => Severity::Error,
            _ => Severity::Warning,
        }
    }
}

/// How a board project holds the column field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnAttachment {
    pub set: Option<i64>,
    pub defaults: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFacts {
    pub id: ProjectId,
    pub archived: bool,
    /// `None` when the project does not have the column field.
    pub column: Option<ColumnAttachment>,
    /// The fields attached to the project.
    pub fields: Vec<FieldId>,
}

/// A column's value with its set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnValue {
    pub id: ValueId,
    pub set: i64,
}

/// A field a setting names; `kind` is `None` when the field is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotField {
    pub slot: Slot,
    pub field: FieldId,
    pub kind: Option<FieldKind>,
    /// The kind is one the setting takes.
    pub fits: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// The board projects that exist.
    pub projects: Vec<ProjectFacts>,
    pub column_field: Option<FieldId>,
    pub columns: Vec<ColumnValue>,
    pub query_mode: bool,
    pub slots: Vec<SlotField>,
    pub backlog_hidden: bool,
}

/// Errors first, then warnings, each in the order of the settings tabs.
pub fn check(facts: &Facts) -> Vec<Finding> {
    let mut out = Vec::new();
    if facts.projects.is_empty() {
        out.push(Finding::NoProjects);
    }
    if facts.column_field.is_none() {
        out.push(Finding::NoColumnField);
    } else {
        if facts.columns.is_empty() {
            out.push(Finding::NoColumns);
        }
        for p in &facts.projects {
            if p.column.is_none() {
                out.push(Finding::ColumnFieldMissing { project: p.id });
            }
        }
    }
    for p in &facts.projects {
        let Some(column) = &p.column else { continue };
        for v in &facts.columns {
            if column.set.is_some_and(|s| s != v.set) {
                out.push(Finding::ColumnValueMissing {
                    project: p.id,
                    value: v.id,
                });
            }
        }
        if facts.query_mode
            && let Some(default) = column.defaults.first()
            && !facts.columns.iter().any(|v| v.id == *default)
        {
            out.push(Finding::DefaultNotColumn { project: p.id });
        }
    }
    for p in &facts.projects {
        if p.archived {
            out.push(Finding::ArchivedProject { project: p.id });
        }
    }
    for s in &facts.slots {
        let attached = facts.projects.iter().any(|p| p.fields.contains(&s.field));
        if s.kind.is_none() || !attached {
            out.push(Finding::FieldMissing { slot: s.slot });
        } else if !s.fits {
            out.push(Finding::FieldWrongKind { slot: s.slot });
        }
    }
    if facts.backlog_hidden {
        out.push(Finding::BacklogHidden);
    }
    out.sort_by_key(|f| f.severity() == Severity::Warning);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: ProjectId, set: Option<i64>, defaults: Vec<i64>) -> ProjectFacts {
        ProjectFacts {
            id,
            archived: false,
            column: Some(ColumnAttachment { set, defaults }),
            fields: vec![10, 20],
        }
    }

    fn healthy() -> Facts {
        Facts {
            projects: vec![project(1, Some(5), vec![100])],
            column_field: Some(10),
            columns: vec![ColumnValue { id: 100, set: 5 }],
            ..Facts::default()
        }
    }

    #[test]
    fn brd_023_healthy_board() {
        assert!(check(&healthy()).is_empty());
    }

    #[test]
    fn brd_023_errors() {
        let none = Facts::default();
        assert_eq!(
            check(&none),
            vec![Finding::NoProjects, Finding::NoColumnField]
        );
        let mut f = healthy();
        f.columns.clear();
        f.projects.push(ProjectFacts {
            column: None,
            ..project(2, None, vec![])
        });
        assert_eq!(
            check(&f),
            vec![
                Finding::NoColumns,
                Finding::ColumnFieldMissing { project: 2 }
            ]
        );
        assert!(check(&f).iter().all(|x| x.severity() == Severity::Error));
    }

    #[test]
    fn brd_023_warnings() {
        let mut f = healthy();
        f.projects.push(ProjectFacts {
            archived: true,
            ..project(2, Some(6), vec![])
        });
        f.query_mode = true;
        f.projects[0].column = Some(ColumnAttachment {
            set: Some(5),
            defaults: vec![101],
        });
        f.slots = vec![
            SlotField {
                slot: Slot::Color,
                field: 30,
                kind: Some(FieldKind::Enum),
                fits: true,
            },
            SlotField {
                slot: Slot::Estimation,
                field: 20,
                kind: Some(FieldKind::String),
                fits: false,
            },
        ];
        f.backlog_hidden = true;
        assert_eq!(
            check(&f),
            vec![
                Finding::DefaultNotColumn { project: 1 },
                Finding::ColumnValueMissing {
                    project: 2,
                    value: 100
                },
                Finding::ArchivedProject { project: 2 },
                Finding::FieldMissing { slot: Slot::Color },
                Finding::FieldWrongKind {
                    slot: Slot::Estimation
                },
                Finding::BacklogHidden,
            ]
        );
    }
}
