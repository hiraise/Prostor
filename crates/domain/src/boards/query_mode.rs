//! The field changes that make an issue match a query board (BRD-028, owner decision 2 of plan
//! 1.8): before an issue joins a board in mode `query` by hand, the person sees what changes and
//! confirms. The walk is pure; the service evaluates the leaves over the issue in one statement
//! and applies the steps, then checks the whole query again.

use crate::fields::FieldKind;
use crate::search::Filter;
use crate::{FieldId, TagId};

/// One change of the issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Set the field to this value (a multi-valued field gets it added).
    Set { field: FieldId, value: i64 },
    /// Empty the field.
    Clear { field: FieldId },
    /// Put the tag on the issue.
    Tag(TagId),
}

/// Why no changes can make the issue match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// A condition that a field change cannot satisfy (a date, a text, a project, a negation…).
    Unsettable,
    /// A condition on a user field names several people (a group): nobody to pick for the person.
    Group,
    /// The condition's field is not attached to the issue's project.
    Detached,
}

impl Failure {
    pub fn message(self) -> &'static str {
        match self {
            Failure::Unsettable => "no field changes make the issue match the board query",
            Failure::Group => "the board query names several people; set the field by hand",
            Failure::Detached => "the board query needs a field the issue's project does not have",
        }
    }
}

/// What the walk needs to know about the issue and the actor.
pub struct Facts<'a> {
    /// Whether each leaf of [`leaves`] holds for the issue now, in the same order.
    pub holds: &'a [bool],
    /// The field is attached to the issue's project.
    pub attached: &'a dyn Fn(FieldId) -> bool,
    /// The actor may put the tag on issues.
    pub usable_tag: &'a dyn Fn(TagId) -> bool,
}

/// The leaves of the filter in walk order: every part that is not an AND or an OR (a negation is
/// one leaf).
pub fn leaves(filter: &Filter) -> Vec<&Filter> {
    fn walk<'a>(f: &'a Filter, out: &mut Vec<&'a Filter>) {
        match f {
            Filter::And(parts) | Filter::Or(parts) => parts.iter().for_each(|p| walk(p, out)),
            other => out.push(other),
        }
    }
    let mut out = Vec::new();
    walk(filter, &mut out);
    out
}

/// The changes that make the issue match: AND needs every part, OR the first part that holds
/// already, else the first that can be made to hold (negations are never made to hold).
pub fn plan(filter: &Filter, facts: &Facts<'_>) -> Result<Vec<Step>, Failure> {
    let mut next = 0;
    walk(filter, facts, &mut next)
}

fn walk(filter: &Filter, facts: &Facts<'_>, next: &mut usize) -> Result<Vec<Step>, Failure> {
    match filter {
        Filter::And(parts) => {
            let mut steps = Vec::new();
            let mut failure = None;
            // Every part is walked, so the leaf counter stays in step.
            for part in parts {
                match walk(part, facts, next) {
                    Ok(s) => steps.extend(s),
                    Err(f) => failure = failure.or(Some(f)),
                }
            }
            failure.map_or(Ok(steps), Err)
        }
        Filter::Or(parts) => {
            let tried: Vec<_> = parts.iter().map(|p| walk(p, facts, next)).collect();
            if tried.iter().any(|t| t.as_ref().is_ok_and(Vec::is_empty)) {
                return Ok(Vec::new());
            }
            let first_failure = tried.iter().find_map(|t| t.as_ref().err().copied());
            tried
                .into_iter()
                .find_map(Result::ok)
                .ok_or(first_failure.unwrap_or(Failure::Unsettable))
        }
        leaf => {
            let held = facts.holds.get(*next).copied().unwrap_or(false);
            *next += 1;
            if held {
                return Ok(Vec::new());
            }
            settle(leaf, facts)
        }
    }
}

fn settle(leaf: &Filter, facts: &Facts<'_>) -> Result<Vec<Step>, Failure> {
    match leaf {
        Filter::True => Ok(Vec::new()),
        Filter::RefValues { field, ids, .. } => {
            if !(facts.attached)(field.id) {
                return Err(Failure::Detached);
            }
            if field.kind == FieldKind::User && ids.len() > 1 {
                return Err(Failure::Group);
            }
            let value = *ids.first().ok_or(Failure::Unsettable)?;
            Ok(vec![Step::Set {
                field: field.id,
                value,
            }])
        }
        Filter::FieldEmpty { field, .. } => {
            if !(facts.attached)(field.id) {
                return Err(Failure::Detached);
            }
            Ok(vec![Step::Clear { field: field.id }])
        }
        Filter::Tags(ids) => ids
            .iter()
            .find(|t| (facts.usable_tag)(**t))
            .map(|t| vec![Step::Tag(*t)])
            .ok_or(Failure::Unsettable),
        _ => Err(Failure::Unsettable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::FieldRef;

    fn field(id: FieldId, kind: FieldKind) -> FieldRef {
        FieldRef {
            id,
            kind,
            multi: false,
            hidden: Vec::new(),
        }
    }

    fn refs(id: FieldId, kind: FieldKind, ids: Vec<i64>) -> Filter {
        Filter::RefValues {
            field: field(id, kind),
            ids,
            implied: None,
        }
    }

    fn run(filter: &Filter, holds: &[bool]) -> Result<Vec<Step>, Failure> {
        let attached = |f: FieldId| f != 99;
        let usable = |t: TagId| t != 13;
        plan(
            filter,
            &Facts {
                holds,
                attached: &attached,
                usable_tag: &usable,
            },
        )
    }

    #[test]
    fn brd_028_plan_and() {
        let f = Filter::And(vec![
            refs(1, FieldKind::User, vec![7]),
            refs(2, FieldKind::Enum, vec![20, 21]),
            Filter::Tags(vec![13, 14]),
            Filter::FieldEmpty {
                field: field(3, FieldKind::Version),
                projects: vec![],
            },
        ]);
        assert_eq!(leaves(&f).len(), 4);
        assert_eq!(
            run(&f, &[false, true, false, false]).unwrap(),
            vec![
                Step::Set { field: 1, value: 7 },
                Step::Tag(14),
                Step::Clear { field: 3 },
            ]
        );
    }

    #[test]
    fn brd_028_plan_or() {
        // The first part cannot hold, the second can: the second's change.
        let f = Filter::Or(vec![
            Filter::Resolved(true),
            refs(2, FieldKind::Enum, vec![20]),
            refs(4, FieldKind::Enum, vec![40]),
        ]);
        assert_eq!(
            run(&f, &[false, false, false]).unwrap(),
            vec![Step::Set {
                field: 2,
                value: 20
            }]
        );
        // A part that holds already wins: no change.
        assert_eq!(run(&f, &[false, false, true]).unwrap(), vec![]);
        // Nested: the counter follows every leaf of a skipped part.
        let nested = Filter::And(vec![
            Filter::Or(vec![
                Filter::And(vec![
                    Filter::Resolved(true),
                    refs(5, FieldKind::Enum, vec![1]),
                ]),
                refs(6, FieldKind::Enum, vec![2]),
            ]),
            refs(7, FieldKind::Enum, vec![3]),
        ]);
        assert_eq!(
            run(&nested, &[false, false, false, true]).unwrap(),
            vec![Step::Set { field: 6, value: 2 }]
        );
    }

    #[test]
    fn brd_028_plan_negation() {
        let not = Filter::Not(Box::new(refs(2, FieldKind::Enum, vec![20])));
        assert_eq!(run(&not, &[false]), Err(Failure::Unsettable));
        assert_eq!(run(&not, &[true]).unwrap(), vec![]);
        // OR skips the negation for the part it can make hold.
        let f = Filter::Or(vec![not, refs(3, FieldKind::Enum, vec![30])]);
        assert_eq!(
            run(&f, &[false, false]).unwrap(),
            vec![Step::Set {
                field: 3,
                value: 30
            }]
        );
    }

    #[test]
    fn brd_028_plan_unsettable_leaf() {
        let f = Filter::And(vec![
            refs(1, FieldKind::Enum, vec![7]),
            Filter::Projects(vec![5]),
        ]);
        assert_eq!(run(&f, &[false, false]), Err(Failure::Unsettable));
        assert_eq!(
            run(&refs(99, FieldKind::Enum, vec![1]), &[false]),
            Err(Failure::Detached)
        );
        assert_eq!(
            run(&Filter::Tags(vec![13]), &[false]),
            Err(Failure::Unsettable)
        );
    }

    #[test]
    fn brd_028_plan_group_value() {
        let f = refs(1, FieldKind::User, vec![7, 8]);
        assert_eq!(run(&f, &[false]), Err(Failure::Group));
        assert_eq!(run(&f, &[true]).unwrap(), vec![]);
    }
}
