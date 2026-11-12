//! Card settings (BRD-059…BRD-063; plan 1.8 §3.16): which fields cards show, how they are
//! colored, which fields estimate them.

use crate::FieldId;
use crate::fields::{FieldKind, VALUE_COLORS};
use crate::validation::{Problem, Rejection};

/// Fields a new board's cards show (BRD-059).
pub const DEFAULT_CARD_FIELDS: usize = 3;

/// The fields shown as a first letter only: some of the card fields, in their order.
pub fn initials(fields: &[FieldId], initials: &[FieldId]) -> Result<Vec<FieldId>, Rejection> {
    if let Some(stray) = initials.iter().find(|f| !fields.contains(f)) {
        return Err(Rejection::new(
            Problem::Invalid,
            format!("field {stray} is not shown on cards"),
        ));
    }
    Ok(fields
        .iter()
        .copied()
        .filter(|f| initials.contains(f))
        .collect())
}

/// A field that colors cards (BRD-060): its values have colors.
pub fn color_field_allowed(kind: FieldKind) -> Result<(), Rejection> {
    match kind {
        FieldKind::Enum
        | FieldKind::State
        | FieldKind::Version
        | FieldKind::Build
        | FieldKind::Owned => Ok(()),
        _ => Err(Rejection::new(
            Problem::Invalid,
            "cards are colored by a field whose values have colors",
        )),
    }
}

/// A field that estimates cards (BRD-062): a number or a duration.
pub fn estimation_field_allowed(kind: FieldKind) -> Result<(), Rejection> {
    match kind {
        FieldKind::Integer | FieldKind::Float | FieldKind::Duration => Ok(()),
        _ => Err(Rejection::new(
            Problem::Invalid,
            "an estimation is an integer, float or duration field",
        )),
    }
}

/// A project's card color (BRD-060): one of the value color schemes (D-028), or none.
pub fn project_color(color: Option<i16>) -> Result<Option<i16>, Rejection> {
    match color {
        Some(c) if !(0..VALUE_COLORS).contains(&c) => Err(Rejection::new(
            Problem::OutOfRange,
            format!("color must be from 0 to {}", VALUE_COLORS - 1),
        )),
        other => Ok(other),
    }
}

/// A field of the first project as the default card fields see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub id: FieldId,
    pub kind: FieldKind,
    /// The built-in assignee: the card shows it as an avatar already.
    pub assignee: bool,
}

/// The card fields of a new board (BRD-059): the first three fields of the first project, except
/// the fields the board uses otherwise (`taken`), the assignee and Markdown text.
pub fn default_fields(first_project: &[Candidate], taken: &[FieldId]) -> Vec<FieldId> {
    first_project
        .iter()
        .filter(|c| !c.assignee && c.kind != FieldKind::Text && !taken.contains(&c.id))
        .map(|c| c.id)
        .take(DEFAULT_CARD_FIELDS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brd_059_initials_and_defaults() {
        assert_eq!(initials(&[3, 1, 2], &[2, 3]).unwrap(), vec![3, 2]);
        assert!(initials(&[1], &[9]).is_err());
        let c = |id, kind, assignee| Candidate { id, kind, assignee };
        let fields = [
            c(1, FieldKind::State, false),
            c(2, FieldKind::User, true),
            c(3, FieldKind::Text, false),
            c(4, FieldKind::Enum, false),
            c(5, FieldKind::Integer, false),
            c(6, FieldKind::Date, false),
            c(7, FieldKind::Version, false),
        ];
        assert_eq!(default_fields(&fields, &[1, 5]), vec![4, 6, 7]);
    }

    #[test]
    fn brd_060_brd_062_kinds() {
        assert!(color_field_allowed(FieldKind::State).is_ok());
        assert!(color_field_allowed(FieldKind::User).is_err());
        assert!(estimation_field_allowed(FieldKind::Duration).is_ok());
        assert!(estimation_field_allowed(FieldKind::String).is_err());
        assert!(project_color(Some(VALUE_COLORS)).is_err());
        assert_eq!(project_color(None).unwrap(), None);
    }
}
