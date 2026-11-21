//! Presets (BRD-003…BRD-007): what a new board starts with, as pure rules over the fields its
//! projects have. Our own rules, no name matching (D-013).

use super::Preset;
use super::columns::Wip;
use crate::fields::FieldKind;
use crate::fields::builtin::BuiltinField;
use crate::{FieldId, ValueId};

/// One value of an attached field's set, in set order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetValue {
    pub id: ValueId,
    pub resolved: bool,
    pub archived: bool,
}

/// A field attached to one project, with the values of the set that project uses (filled for
/// state fields; others may leave it empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    pub field: FieldId,
    pub kind: FieldKind,
    pub multi: bool,
    pub builtin: Option<BuiltinField>,
    pub values: Vec<SetValue>,
}

/// The fields of one board project, in the project's order.
pub type ProjectFields = Vec<Attached>;

/// Everything a preset decides; the columns carry their WIP limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    pub sprints_on: bool,
    pub column_field: Option<FieldId>,
    pub columns: Vec<(ValueId, Wip)>,
    pub color_field: Option<FieldId>,
    pub estimation_field: Option<FieldId>,
}

pub fn setup(preset: Preset, projects: &[ProjectFields]) -> Setup {
    let ids = columns(preset, projects);
    let wip = wip(preset, ids.len());
    Setup {
        sprints_on: preset != Preset::Kanban,
        column_field: match preset {
            Preset::Custom => None,
            _ => column_field(projects),
        },
        columns: ids.into_iter().zip(wip).collect(),
        color_field: match preset {
            Preset::Custom => None,
            _ => builtin_anywhere(projects, BuiltinField::Priority),
        },
        estimation_field: match preset {
            Preset::Custom => None,
            _ => builtin_everywhere(projects, BuiltinField::Estimation),
        },
    }
}

fn in_every(projects: &[ProjectFields], field: FieldId) -> bool {
    projects
        .iter()
        .all(|fields| fields.iter().any(|f| f.field == field))
}

fn builtin_everywhere(projects: &[ProjectFields], key: BuiltinField) -> Option<FieldId> {
    let first = projects.first()?;
    let field = first.iter().find(|f| f.builtin == Some(key))?.field;
    in_every(projects, field).then_some(field)
}

fn builtin_anywhere(projects: &[ProjectFields], key: BuiltinField) -> Option<FieldId> {
    projects
        .iter()
        .flatten()
        .find(|f| f.builtin == Some(key))
        .map(|f| f.field)
}

/// The state field every board project has: the built-in `state` first, else the first
/// single-value state field (in the first project's order) attached to every project.
pub fn column_field(projects: &[ProjectFields]) -> Option<FieldId> {
    let first = projects.first()?;
    let shared =
        |f: &&Attached| f.kind == FieldKind::State && !f.multi && in_every(projects, f.field);
    first
        .iter()
        .filter(shared)
        .find(|f| f.builtin == Some(BuiltinField::State))
        .or_else(|| first.iter().find(shared))
        .map(|f| f.field)
}

/// Scrum and kanban: every unresolved value of the column field's set, in the first project's
/// set order, that every project's set holds; then the first such resolved value. Archived values
/// are left out. Custom: none.
pub fn columns(preset: Preset, projects: &[ProjectFields]) -> Vec<ValueId> {
    if preset == Preset::Custom {
        return Vec::new();
    }
    let Some(field) = column_field(projects) else {
        return Vec::new();
    };
    let sets: Vec<&[SetValue]> = projects
        .iter()
        .filter_map(|fields| fields.iter().find(|f| f.field == field))
        .map(|f| f.values.as_slice())
        .collect();
    let held = |v: &&SetValue| !v.archived && sets.iter().all(|s| s.iter().any(|x| x.id == v.id));
    let first = sets[0];
    let mut out: Vec<ValueId> = first
        .iter()
        .filter(held)
        .filter(|v| !v.resolved)
        .map(|v| v.id)
        .collect();
    out.extend(first.iter().filter(held).find(|v| v.resolved).map(|v| v.id));
    out
}

/// WIP limits of `n` new columns: kanban caps every column but the first and the last at 5.
pub fn wip(preset: Preset, n: usize) -> Vec<Wip> {
    (0..n)
        .map(|i| match preset {
            Preset::Kanban if i > 0 && i + 1 < n => Wip {
                min: None,
                max: Some(5),
            },
            _ => Wip::default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: ValueId, resolved: bool) -> SetValue {
        SetValue {
            id,
            resolved,
            archived: false,
        }
    }

    fn state(field: FieldId, builtin: bool, values: Vec<SetValue>) -> Attached {
        Attached {
            field,
            kind: FieldKind::State,
            multi: false,
            builtin: builtin.then_some(BuiltinField::State),
            values,
        }
    }

    fn plain(field: FieldId, kind: FieldKind, builtin: Option<BuiltinField>) -> Attached {
        Attached {
            field,
            kind,
            multi: false,
            builtin,
            values: vec![],
        }
    }

    fn standard() -> Vec<SetValue> {
        vec![
            v(1, false),
            v(2, false),
            v(3, false),
            v(4, true),
            v(5, true),
        ]
    }

    #[test]
    fn brd_004_columns() {
        let p = vec![state(10, true, standard())];
        assert_eq!(
            columns(Preset::Scrum, std::slice::from_ref(&p)),
            vec![1, 2, 3, 4]
        );
        assert_eq!(
            columns(Preset::Kanban, std::slice::from_ref(&p)),
            vec![1, 2, 3, 4]
        );
        assert!(columns(Preset::Custom, &[p]).is_empty());
    }

    #[test]
    fn brd_004_values_every_project_holds() {
        let mut archived = standard();
        archived[1].archived = true;
        let a = vec![state(10, true, archived)];
        let b = vec![state(10, true, vec![v(3, false), v(1, false), v(5, true)])];
        // Order of the first project's set; 2 is archived, 4 is not in b's set.
        assert_eq!(columns(Preset::Scrum, &[a, b]), vec![1, 3, 5]);
    }

    #[test]
    fn brd_004_column_field_choice() {
        // The built-in state field wins over an earlier custom one.
        let p = vec![state(20, false, standard()), state(10, true, standard())];
        assert_eq!(column_field(&[p]), Some(10));
        // Without it on every project: the first state field every project has.
        let a = vec![state(10, true, standard()), state(20, false, standard())];
        let b = vec![state(20, false, standard())];
        assert_eq!(column_field(&[a, b]), Some(20));
        // None shared: no columns (a health error later).
        let a = vec![state(10, true, standard())];
        let b = vec![state(20, false, standard())];
        assert_eq!(column_field(&[a.clone(), b.clone()]), None);
        assert!(columns(Preset::Scrum, &[a, b]).is_empty());
        assert_eq!(column_field(&[]), None);
    }

    #[test]
    fn brd_005_wip() {
        assert_eq!(wip(Preset::Scrum, 3), vec![Wip::default(); 3]);
        let k = wip(Preset::Kanban, 4);
        assert_eq!(k[0], Wip::default());
        assert_eq!(k[1].max, Some(5));
        assert_eq!(k[2].max, Some(5));
        assert_eq!(k[3], Wip::default());
        assert_eq!(wip(Preset::Kanban, 2), vec![Wip::default(); 2]);
        assert!(wip(Preset::Kanban, 0).is_empty());
    }

    #[test]
    fn brd_003_setup() {
        let a = vec![
            state(10, true, standard()),
            plain(30, FieldKind::Enum, Some(BuiltinField::Priority)),
            plain(40, FieldKind::Duration, Some(BuiltinField::Estimation)),
        ];
        let b = vec![
            state(10, true, standard()),
            plain(40, FieldKind::Duration, Some(BuiltinField::Estimation)),
        ];
        let scrum = setup(Preset::Scrum, &[a.clone(), b.clone()]);
        assert!(scrum.sprints_on);
        assert_eq!(scrum.column_field, Some(10));
        assert_eq!(scrum.columns.len(), 4);
        assert_eq!(scrum.color_field, Some(30));
        assert_eq!(scrum.estimation_field, Some(40));
        let kanban = setup(Preset::Kanban, &[a.clone(), b.clone()]);
        assert!(!kanban.sprints_on);
        assert_eq!(kanban.estimation_field, Some(40));
        assert_eq!(kanban.columns[1].1.max, Some(5));
        let custom = setup(Preset::Custom, &[a, b]);
        assert!(custom.sprints_on);
        assert_eq!(custom.column_field, None);
        assert!(custom.columns.is_empty());
        assert_eq!(custom.color_field, None);
    }
}
