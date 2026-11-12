//! Columns (BRD-042…BRD-046): which fields may define them and the WIP limits.

use crate::fields::FieldKind;
use crate::validation::{Problem, Rejection};

/// Highest WIP limit.
pub const WIP_MAX: i32 = 999;

/// Validates a column field (BRD-042): one single-value field of a kind whose values are a list —
/// state, enum, owned or version. A card cannot be in two columns.
pub fn field_allowed(kind: FieldKind, multi: bool) -> Result<(), Rejection> {
    if multi {
        return Err(Rejection::new(
            Problem::Invalid,
            "a field with several values cannot define columns",
        ));
    }
    match kind {
        FieldKind::State | FieldKind::Enum | FieldKind::Owned | FieldKind::Version => Ok(()),
        _ => Err(Rejection::new(
            Problem::Invalid,
            "columns come from a state, enum, owned or version field",
        )),
    }
}

/// One more column on a board that has `count`.
pub fn may_add(count: usize) -> Result<(), Rejection> {
    if count >= super::COLUMNS_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("at most {} columns", super::COLUMNS_MAX),
        ));
    }
    Ok(())
}

/// Minimum and maximum card counts of a column (BRD-046); none by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Wip {
    pub min: Option<i32>,
    pub max: Option<i32>,
}

impl Wip {
    /// Each limit 0…[`WIP_MAX`], the minimum not above the maximum.
    pub fn new(min: Option<i32>, max: Option<i32>) -> Result<Self, Rejection> {
        for limit in [min, max].into_iter().flatten() {
            if !(0..=WIP_MAX).contains(&limit) {
                return Err(Rejection::new(
                    Problem::OutOfRange,
                    format!("WIP limits are 0…{WIP_MAX}"),
                ));
            }
        }
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return Err(Rejection::new(
                Problem::OutOfRange,
                "the minimum is above the maximum",
            ));
        }
        Ok(Self { min, max })
    }

    /// Where a column with `count` cards stands; limits warn, never block moves.
    pub fn state(self, count: i64) -> WipState {
        if self.max.is_some_and(|max| count > i64::from(max)) {
            WipState::Over
        } else if self.min.is_some_and(|min| count < i64::from(min)) {
            WipState::Under
        } else {
            WipState::Within
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WipState {
    Within,
    Under,
    Over,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brd_042_column_field_kinds() {
        for kind in [
            FieldKind::State,
            FieldKind::Enum,
            FieldKind::Owned,
            FieldKind::Version,
        ] {
            assert!(field_allowed(kind, false).is_ok(), "{kind:?}");
        }
        assert!(field_allowed(FieldKind::Enum, true).is_err(), "multi");
        for kind in [FieldKind::User, FieldKind::String, FieldKind::Build] {
            assert!(field_allowed(kind, false).is_err(), "{kind:?}");
        }
        assert!(may_add(super::super::COLUMNS_MAX - 1).is_ok());
        assert!(may_add(super::super::COLUMNS_MAX).is_err());
    }

    #[test]
    fn brd_046_wip_rules() {
        assert_eq!(Wip::new(None, None).unwrap(), Wip::default());
        assert!(Wip::new(Some(0), Some(999)).is_ok());
        assert!(Wip::new(Some(2), Some(2)).is_ok());
        assert!(Wip::new(Some(3), Some(2)).is_err(), "min above max");
        assert!(Wip::new(None, Some(1000)).is_err());
        assert!(Wip::new(Some(-1), None).is_err());
        let w = Wip::new(Some(1), Some(3)).unwrap();
        assert_eq!(w.state(0), WipState::Under);
        assert_eq!(w.state(1), WipState::Within);
        assert_eq!(w.state(3), WipState::Within);
        assert_eq!(w.state(4), WipState::Over);
        assert_eq!(Wip::default().state(500), WipState::Within);
    }
}
