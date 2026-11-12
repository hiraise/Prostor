//! Sprints (BRD-097…BRD-104): when one is finished, which one is current, default names. Dates
//! are calendar days in the instance time zone; `today` comes from the caller. Nothing runs on a
//! schedule: all is computed on read (D-013).

use time::{Date, OffsetDateTime};

use crate::SprintId;
use crate::settings::Language;
use crate::validation::{Problem, Rejection};

/// What detection needs of a sprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SprintDates {
    pub id: SprintId,
    pub starts_on: Option<Date>,
    pub ends_on: Option<Date>,
    pub finished_at: Option<OffsetDateTime>,
    pub archived: bool,
    pub internal: bool,
    pub created_at: OffsetDateTime,
}

/// Finished by hand, or today is after the last day (BRD-100: a sprint ends at the end of its
/// finish date).
pub fn finished(sprint: &SprintDates, today: Date) -> bool {
    sprint.finished_at.is_some() || sprint.ends_on.is_some_and(|end| today > end)
}

/// The current sprint (BRD-099), never set by hand. Among the non-archived, non-internal,
/// unfinished sprints, scheduled ones (with a start date) go first: the earliest start wins, a tie
/// goes to the latest end (an open end counts as the latest), then to the earliest created.
/// Unscheduled sprints compete only when no scheduled one is left: the earliest created wins.
/// `None` when no sprint qualifies; a sprintless board shows its internal sprint instead.
pub fn current(sprints: &[SprintDates], today: Date) -> Option<SprintId> {
    let open = sprints
        .iter()
        .filter(|s| !s.archived && !s.internal && !finished(s, today));
    let created = |s: &SprintDates| (s.created_at, s.id);
    let scheduled = open
        .clone()
        .filter_map(|s| s.starts_on.map(|start| (start, s)))
        .min_by(|(a_start, a), (b_start, b)| {
            a_start
                .cmp(b_start)
                // Latest end first: `None` (open end) is the latest.
                .then_with(|| match (a.ends_on, b.ends_on) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (Some(x), Some(y)) => y.cmp(&x),
                })
                .then_with(|| created(a).cmp(&created(b)))
        })
        .map(|(_, s)| s.id);
    scheduled.or_else(|| open.min_by_key(|s| created(s)).map(|s| s.id))
}

/// The sprint a board shows when none is asked for: the internal sprint of a sprintless board,
/// else the current one.
pub fn shown(sprints_on: bool, sprints: &[SprintDates], today: Date) -> Option<SprintId> {
    if !sprints_on && let Some(internal) = sprints.iter().find(|s| s.internal) {
        return Some(internal.id);
    }
    current(sprints, today)
}

/// Name of the `n`-th sprint in the instance language: «Спринт 1», "Sprint 1" (BRD-098).
pub fn numbered_name(language: Language, n: usize) -> String {
    match language {
        Language::En => format!("Sprint {n}"),
        Language::Ru => format!("Спринт {n}"),
    }
}

/// Validates a schedule (BRD-097): unscheduled, a start with an open end, or a start and an end
/// on or after it. An end needs a start.
pub fn schedule(starts_on: Option<Date>, ends_on: Option<Date>) -> Result<(), Rejection> {
    match (starts_on, ends_on) {
        (None, Some(_)) => Err(Rejection::new(
            Problem::Required,
            "a sprint with an end needs a start",
        )),
        (Some(start), Some(end)) if end < start => Err(Rejection::new(
            Problem::OutOfRange,
            "the sprint ends before it starts",
        )),
        _ => Ok(()),
    }
}

/// The dates of a sprint started by hand today (D-013): the start moves to today when it is later
/// or empty and an end already passed goes; the caller undoes a finish by hand.
pub fn started(
    starts_on: Option<Date>,
    ends_on: Option<Date>,
    today: Date,
) -> (Date, Option<Date>) {
    let start = starts_on.filter(|d| *d <= today).unwrap_or(today);
    (start, ends_on.filter(|end| *end >= today))
}

/// The first free numbered name «Спринт N» (BRD-024, BRD-098), counting from `taken.len() + 1`;
/// names compare ignoring case.
pub fn free_numbered_name(language: Language, taken: &[&str]) -> String {
    let lower: Vec<String> = taken.iter().map(|t| t.to_lowercase()).collect();
    (taken.len() + 1..)
        .map(|n| numbered_name(language, n))
        .find(|name| !lower.contains(&name.to_lowercase()))
        .unwrap_or_else(|| numbered_name(language, taken.len() + 1))
}

/// Name of the hidden sprint of a sprintless board; never shown, unique by the internal flag.
pub const INTERNAL_NAME: &str = "internal";

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    const TODAY: Date = date!(2026 - 10 - 24);

    fn sprint(id: SprintId) -> SprintDates {
        SprintDates {
            id,
            starts_on: None,
            ends_on: None,
            finished_at: None,
            archived: false,
            internal: false,
            created_at: datetime!(2026-01-01 0:00 UTC) + time::Duration::days(id),
        }
    }

    fn dated(id: SprintId, start: Date, end: Option<Date>) -> SprintDates {
        SprintDates {
            starts_on: Some(start),
            ends_on: end,
            ..sprint(id)
        }
    }

    #[test]
    fn brd_097_schedule() {
        assert!(schedule(None, None).is_ok());
        assert!(schedule(Some(TODAY), None).is_ok());
        assert!(schedule(Some(TODAY), Some(TODAY)).is_ok());
        assert_eq!(
            schedule(None, Some(TODAY)).unwrap_err().problem,
            Problem::Required
        );
        assert_eq!(
            schedule(Some(TODAY), Some(date!(2026 - 10 - 23)))
                .unwrap_err()
                .problem,
            Problem::OutOfRange
        );
    }

    #[test]
    fn brd_097_started_by_hand() {
        assert_eq!(started(None, None, TODAY), (TODAY, None));
        let later = date!(2026 - 11 - 01);
        assert_eq!(
            started(Some(later), Some(later), TODAY),
            (TODAY, Some(later))
        );
        let earlier = date!(2026 - 10 - 01);
        assert_eq!(
            started(Some(earlier), Some(date!(2026 - 10 - 10)), TODAY),
            (earlier, None),
            "an end already passed goes"
        );
    }

    #[test]
    fn brd_024_free_numbered_name() {
        assert_eq!(free_numbered_name(Language::En, &[]), "Sprint 1");
        assert_eq!(
            free_numbered_name(Language::Ru, &["Спринт 1", "спринт 3", "Q4"]),
            "Спринт 4"
        );
        assert_eq!(free_numbered_name(Language::En, &["sprint 2"]), "Sprint 3");
    }

    #[test]
    fn brd_100_finished() {
        let mut s = dated(1, date!(2026 - 10 - 01), Some(TODAY));
        assert!(!finished(&s, TODAY), "the last day still runs");
        assert!(finished(&s, date!(2026 - 10 - 25)));
        s.ends_on = None;
        assert!(!finished(&s, TODAY));
        s.finished_at = Some(datetime!(2026-10-02 12:00 UTC));
        assert!(finished(&s, TODAY), "finished by hand");
    }

    #[test]
    fn brd_099_earliest_start_wins() {
        let s = [
            dated(1, date!(2026 - 10 - 20), Some(date!(2026 - 11 - 01))),
            dated(2, date!(2026 - 10 - 10), Some(date!(2026 - 10 - 30))),
            dated(3, date!(2026 - 11 - 10), None),
        ];
        assert_eq!(current(&s, TODAY), Some(2));
    }

    #[test]
    fn brd_099_tie_goes_to_latest_end() {
        let s = [
            dated(1, date!(2026 - 10 - 10), Some(date!(2026 - 10 - 30))),
            dated(2, date!(2026 - 10 - 10), Some(date!(2026 - 11 - 15))),
        ];
        assert_eq!(current(&s, TODAY), Some(2));
        let open = [
            dated(1, date!(2026 - 10 - 10), Some(date!(2026 - 11 - 15))),
            dated(2, date!(2026 - 10 - 10), None),
        ];
        assert_eq!(current(&open, TODAY), Some(2), "an open end is the latest");
        let same = [
            dated(4, date!(2026 - 10 - 10), None),
            dated(3, date!(2026 - 10 - 10), None),
        ];
        assert_eq!(current(&same, TODAY), Some(3), "then the earliest created");
    }

    #[test]
    fn brd_099_finished_archived_internal_left_out() {
        let mut finished_by_hand = dated(1, date!(2026 - 10 - 01), None);
        finished_by_hand.finished_at = Some(datetime!(2026-10-05 0:00 UTC));
        let ended = dated(2, date!(2026 - 10 - 02), Some(date!(2026 - 10 - 20)));
        let mut archived = dated(3, date!(2026 - 10 - 03), None);
        archived.archived = true;
        let mut internal = sprint(4);
        internal.internal = true;
        let later = dated(5, date!(2026 - 12 - 01), None);
        let s = [finished_by_hand, ended, archived, internal, later];
        assert_eq!(current(&s, TODAY), Some(5), "a future sprint is current");
    }

    #[test]
    fn brd_099_unscheduled_only_when_all_are() {
        let s = [sprint(1), dated(2, date!(2026 - 12 - 01), None), sprint(3)];
        assert_eq!(current(&s, TODAY), Some(2), "scheduled ones go first");
        let all = [sprint(3), sprint(1), sprint(2)];
        assert_eq!(current(&all, TODAY), Some(1), "the earliest created");
        assert_eq!(current(&[], TODAY), None);
    }

    #[test]
    fn brd_024_sprintless_board_shows_internal() {
        let mut internal = sprint(9);
        internal.internal = true;
        let s = [sprint(1), internal];
        assert_eq!(shown(false, &s, TODAY), Some(9));
        assert_eq!(shown(true, &s, TODAY), Some(1));
        assert_eq!(numbered_name(Language::Ru, 1), "Спринт 1");
        assert_eq!(numbered_name(Language::En, 2), "Sprint 2");
    }
}
