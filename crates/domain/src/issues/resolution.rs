//! Resolved state (ISS-023, FLD-009, FLD-046): an issue is resolved when every state field of its
//! project holds a value flagged as resolved. A project without state fields has no resolved
//! issues. The resolution time is set when an issue becomes resolved and cleared when it reopens.

use time::OffsetDateTime;

use crate::{FieldId, ValueId};

/// One state field of the project and the value the issue holds in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateSlot {
    pub field: FieldId,
    pub value: Option<ValueId>,
}

/// Whether the issue is resolved, given which values are flagged resolved.
pub fn is_resolved(slots: &[StateSlot], resolved: impl Fn(ValueId) -> bool) -> bool {
    !slots.is_empty() && slots.iter().all(|slot| slot.value.is_some_and(&resolved))
}

/// The resolution time after a change: kept while the issue stays resolved, set to `now` when it
/// becomes resolved, cleared when it is not.
pub fn resolved_at(
    before: Option<OffsetDateTime>,
    resolved: bool,
    now: OffsetDateTime,
) -> Option<OffsetDateTime> {
    match (before, resolved) {
        (Some(at), true) => Some(at),
        (None, true) => Some(now),
        (_, false) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn iss_023_every_state_field_must_be_resolved() {
        let done = |value: ValueId| value >= 10;
        let slot = |field, value| StateSlot { field, value };
        assert!(!is_resolved(&[], done));
        assert!(is_resolved(&[slot(1, Some(10))], done));
        assert!(!is_resolved(&[slot(1, Some(3))], done));
        assert!(!is_resolved(&[slot(1, None)], done));
        assert!(!is_resolved(&[slot(1, Some(10)), slot(2, Some(3))], done));
        assert!(is_resolved(&[slot(1, Some(10)), slot(2, Some(11))], done));
    }

    #[test]
    fn iss_023_resolution_time() {
        let then = datetime!(2026-01-01 0:00 UTC);
        let now = datetime!(2026-10-22 12:00 UTC);
        assert_eq!(resolved_at(None, true, now), Some(now));
        assert_eq!(resolved_at(Some(then), true, now), Some(then));
        assert_eq!(resolved_at(Some(then), false, now), None);
        assert_eq!(resolved_at(None, false, now), None);
    }
}
