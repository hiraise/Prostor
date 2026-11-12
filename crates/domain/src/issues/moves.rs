//! Field values when an issue changes project (ISS-025, FLD-059): a field the target lacks is
//! erased; a value the target's set holds is kept; a value it lacks gives way to the target's
//! default. Required checks are skipped. The same plan shows the preview before a move.

use super::values::{Value, default_value};
use crate::FieldId;

/// What the target project says about one field the issue has a value in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetField {
    /// The references of the issue's value that the target's set accepts, in their order. Ignored
    /// for kinds without references, whose values move as they are.
    pub accepts: Vec<i64>,
    /// The target's default values for the field.
    pub defaults: Vec<i64>,
}

/// What happens to one value.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Moves as it is.
    Kept,
    /// The target has no such field: the value is gone, also if the issue moves back.
    Erased,
    /// Some or all references are not in the target's set: what is left, else the target's
    /// default, else nothing.
    Replaced(Option<Value>),
}

/// Plans the values of a move. `target` answers for a field of the issue: `None` when the target
/// project does not have it.
pub fn plan(
    current: &[(FieldId, Value)],
    target: impl Fn(FieldId, &Value) -> Option<TargetField>,
) -> Vec<(FieldId, Outcome)> {
    current
        .iter()
        .map(|(field, value)| {
            let outcome = match (target(*field, value), value) {
                (None, _) => Outcome::Erased,
                (Some(there), Value::Refs(ids)) => {
                    let kept: Vec<i64> = ids
                        .iter()
                        .filter(|id| there.accepts.contains(id))
                        .copied()
                        .collect();
                    if kept.len() == ids.len() {
                        Outcome::Kept
                    } else if kept.is_empty() {
                        Outcome::Replaced(default_value(&there.defaults))
                    } else {
                        Outcome::Replaced(Some(Value::Refs(kept)))
                    }
                }
                (Some(_), _) => Outcome::Kept,
            };
            (*field, outcome)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fld_059_values_on_a_move() {
        let current = vec![
            (1, Value::Refs(vec![10])),
            (2, Value::Refs(vec![20, 21])),
            (3, Value::Refs(vec![30])),
            (4, Value::Integer(5)),
            (5, Value::Refs(vec![50])),
            (6, Value::Refs(vec![60])),
        ];
        let plan = plan(&current, |field, _| match field {
            1 => Some(TargetField {
                accepts: vec![10],
                defaults: vec![],
            }),
            2 => Some(TargetField {
                accepts: vec![21],
                defaults: vec![],
            }),
            3 => Some(TargetField {
                accepts: vec![],
                defaults: vec![31],
            }),
            4 => Some(TargetField {
                accepts: vec![],
                defaults: vec![],
            }),
            6 => Some(TargetField {
                accepts: vec![],
                defaults: vec![],
            }),
            _ => None,
        });
        assert_eq!(
            plan,
            vec![
                (1, Outcome::Kept),
                (2, Outcome::Replaced(Some(Value::Refs(vec![21])))),
                (3, Outcome::Replaced(Some(Value::Refs(vec![31])))),
                (4, Outcome::Kept),
                (5, Outcome::Erased),
                (6, Outcome::Replaced(None)),
            ]
        );
    }
}
