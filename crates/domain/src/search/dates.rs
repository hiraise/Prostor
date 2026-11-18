//! Dates in queries (plan §3.4): literals and date words become periods of wall-clock time in a
//! zone. Weeks start on Monday; working days are Monday to Friday.

use jiff::civil::{Date, DateTime, Time, Weekday};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};

/// A period of wall-clock time, `[start, end)`; `None` is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub start: Option<DateTime>,
    pub end: Option<DateTime>,
}

/// The clock queries are read with.
#[derive(Debug, Clone)]
pub struct Clock {
    pub now: Timestamp,
    pub zone: TimeZone,
}

impl Clock {
    fn now(&self) -> DateTime {
        self.now.to_zoned(self.zone.clone()).datetime()
    }

    pub(crate) fn today(&self) -> Date {
        self.now().date()
    }

    /// An instant of this zone's wall-clock time; gaps and folds resolve as the zone's
    /// "compatible" rule says (the later instant of a gap, the earlier of a fold).
    pub fn instant(&self, at: DateTime) -> Option<Timestamp> {
        at.to_zoned(self.zone.clone()).ok().map(|z| z.timestamp())
    }

    /// The period a date word names (its key in `taskmaster_query::words`).
    pub fn word(&self, key: &str) -> Option<Period> {
        let today = self.today();
        let monday = today
            .checked_sub(i64::from(today.weekday().to_monday_zero_offset()).days())
            .ok()?;
        let first = today.first_of_month();
        let weeks = |from: i64, count: i64| -> Option<Period> {
            let start = monday.checked_add((7 * from).days()).ok()?;
            days(start, 7 * count)
        };
        let months = |from: i32| -> Option<Period> {
            let start = first.checked_add(from.months()).ok()?;
            Some(Period {
                start: Some(midnight(start)),
                end: Some(midnight(start.checked_add(1.month()).ok()?)),
            })
        };
        match key {
            "now" => {
                let now = self.now();
                let minute = now.date().at(now.hour(), now.minute(), 0, 0);
                Some(Period {
                    start: Some(minute),
                    end: Some(minute.checked_add(1.minute()).ok()?),
                })
            }
            "today" => days(today, 1),
            "yesterday" => days(today.yesterday().ok()?, 1),
            "tomorrow" => days(today.tomorrow().ok()?, 1),
            "this_week" => weeks(0, 1),
            "last_week" => weeks(-1, 1),
            "next_week" => weeks(1, 1),
            "two_weeks_ago" => weeks(-2, 1),
            "three_weeks_ago" => weeks(-3, 1),
            "this_month" => months(0),
            "last_month" => months(-1),
            "next_month" => months(1),
            "older" => Some(Period {
                start: None,
                end: Some(midnight(first.checked_sub(1.month()).ok()?)),
            }),
            "last_working_day" => {
                let mut day = today.yesterday().ok()?;
                while matches!(day.weekday(), Weekday::Saturday | Weekday::Sunday) {
                    day = day.yesterday().ok()?;
                }
                days(day, 1)
            }
            weekday => {
                let offset = WEEKDAYS.iter().position(|w| *w == weekday)?;
                days(monday.checked_add((offset as i64).days()).ok()?, 1)
            }
        }
    }

    /// A literal: `2026-10-23`, `2026-10`, `10-23` (this year), `2026-10-23T14:30[:05]`.
    pub fn literal(&self, text: &str) -> Option<Period> {
        let (date, time) = match text.split_once(['T', 't']) {
            Some((date, time)) => (date, Some(time)),
            None => (text, None),
        };
        let parts: Vec<&str> = date.split('-').collect();
        let number = |s: &str, len: usize| -> Option<i16> {
            (s.len() == len && s.bytes().all(|b| b.is_ascii_digit()))
                .then(|| s.parse().ok())
                .flatten()
        };
        let day = match parts.as_slice() {
            [y, m, d] => Date::new(
                number(y, 4)?,
                i8::try_from(number(m, 2)?).ok()?,
                i8::try_from(number(d, 2)?).ok()?,
            )
            .ok()?,
            [y, m] if y.len() == 4 => {
                if time.is_some() {
                    return None;
                }
                let start = Date::new(number(y, 4)?, i8::try_from(number(m, 2)?).ok()?, 1).ok()?;
                return Some(Period {
                    start: Some(midnight(start)),
                    end: Some(midnight(start.checked_add(1.month()).ok()?)),
                });
            }
            [m, d] => Date::new(
                self.today().year(),
                i8::try_from(number(m, 2)?).ok()?,
                i8::try_from(number(d, 2)?).ok()?,
            )
            .ok()?,
            _ => return None,
        };
        let Some(time) = time else {
            return days(day, 1);
        };
        let fields: Vec<&str> = time.split(':').collect();
        let (h, m, s, step) = match fields.as_slice() {
            [h, m] => (number(h, 2)?, number(m, 2)?, 0, 1.minute()),
            [h, m, s] => (number(h, 2)?, number(m, 2)?, number(s, 2)?, 1.second()),
            _ => return None,
        };
        let at = Time::new(
            i8::try_from(h).ok()?,
            i8::try_from(m).ok()?,
            i8::try_from(s).ok()?,
            0,
        )
        .ok()?;
        let start = day.to_datetime(at);
        Some(Period {
            start: Some(start),
            end: Some(start.checked_add(step).ok()?),
        })
    }
}

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

fn midnight(date: Date) -> DateTime {
    date.to_datetime(Time::midnight())
}

fn days(start: Date, count: i64) -> Option<Period> {
    Some(Period {
        start: Some(midnight(start)),
        end: Some(midnight(start.checked_add(count.days()).ok()?)),
    })
}

/// The calendar date a wall-clock time falls on, rounded up past midnight (period ends).
pub fn date_ceil(at: DateTime) -> Option<Date> {
    if at.time() == Time::midnight() {
        Some(at.date())
    } else {
        at.date().tomorrow().ok()
    }
}

pub fn to_time_date(date: Date) -> Option<time::Date> {
    let month = time::Month::try_from(u8::try_from(date.month()).ok()?).ok()?;
    time::Date::from_calendar_date(
        i32::from(date.year()),
        month,
        u8::try_from(date.day()).ok()?,
    )
    .ok()
}

pub fn to_offset(at: Timestamp) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::from_unix_timestamp_nanos(at.as_nanosecond()).ok()
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    /// Thursday 2026-10-22, 14:05 in Berlin (summer time until 2026-10-25).
    fn clock() -> Clock {
        clock_at("2026-10-22T12:05:00Z")
    }

    fn clock_at(now: &str) -> Clock {
        Clock {
            now: now.parse().unwrap(),
            zone: TimeZone::get("Europe/Berlin").unwrap(),
        }
    }

    fn period(start: Option<Date>, end: Option<Date>) -> Period {
        Period {
            start: start.map(midnight),
            end: end.map(midnight),
        }
    }

    #[test]
    fn days_and_weeks_start_on_monday() {
        let c = clock();
        let d = |m, d| Some(date(2026, m, d));
        assert_eq!(c.word("today"), Some(period(d(10, 22), d(10, 23))));
        assert_eq!(c.word("yesterday"), Some(period(d(10, 21), d(10, 22))));
        assert_eq!(c.word("tomorrow"), Some(period(d(10, 23), d(10, 24))));
        assert_eq!(c.word("monday"), Some(period(d(10, 19), d(10, 20))));
        assert_eq!(c.word("sunday"), Some(period(d(10, 25), d(10, 26))));
        assert_eq!(c.word("this_week"), Some(period(d(10, 19), d(10, 26))));
        assert_eq!(c.word("last_week"), Some(period(d(10, 12), d(10, 19))));
        assert_eq!(c.word("next_week"), Some(period(d(10, 26), d(11, 2))));
        assert_eq!(c.word("two_weeks_ago"), Some(period(d(10, 5), d(10, 12))));
        assert_eq!(c.word("three_weeks_ago"), Some(period(d(9, 28), d(10, 5))));
        assert_eq!(c.word("unknown"), None);
    }

    #[test]
    fn months_and_older() {
        let c = clock_at("2026-01-15T10:00:00Z");
        let d = |y, m, d| Some(date(y, m, d));
        assert_eq!(
            c.word("this_month"),
            Some(period(d(2026, 1, 1), d(2026, 2, 1)))
        );
        assert_eq!(
            c.word("last_month"),
            Some(period(d(2025, 12, 1), d(2026, 1, 1)))
        );
        assert_eq!(
            c.word("next_month"),
            Some(period(d(2026, 2, 1), d(2026, 3, 1)))
        );
        // Ends where last month starts: the month two months back is the last one inside.
        assert_eq!(c.word("older"), Some(period(None, d(2025, 12, 1))));
    }

    #[test]
    fn year_end_and_now() {
        // 23:30 UTC on 2026-12-31 is already 2027 in Berlin.
        let c = clock_at("2026-12-31T23:30:00Z");
        assert_eq!(
            c.word("today"),
            Some(period(Some(date(2027, 1, 1)), Some(date(2027, 1, 2))))
        );
        assert_eq!(
            c.word("now"),
            Some(Period {
                start: Some(date(2027, 1, 1).at(0, 30, 0, 0)),
                end: Some(date(2027, 1, 1).at(0, 31, 0, 0)),
            })
        );
    }

    #[test]
    fn last_working_day_skips_the_weekend() {
        // Monday 2026-10-26: the last working day is Friday.
        let c = clock_at("2026-10-26T09:00:00Z");
        assert_eq!(
            c.word("last_working_day"),
            Some(period(Some(date(2026, 10, 23)), Some(date(2026, 10, 24))))
        );
        // Wednesday: Tuesday.
        let c = clock_at("2026-10-28T09:00:00Z");
        assert_eq!(
            c.word("last_working_day"),
            Some(period(Some(date(2026, 10, 27)), Some(date(2026, 10, 28))))
        );
    }

    #[test]
    fn literals() {
        let c = clock();
        let d = |y, m, d| Some(date(y, m, d));
        assert_eq!(
            c.literal("2026-02-28"),
            Some(period(d(2026, 2, 28), d(2026, 3, 1)))
        );
        assert_eq!(
            c.literal("2026-02"),
            Some(period(d(2026, 2, 1), d(2026, 3, 1)))
        );
        assert_eq!(
            c.literal("2025-12"),
            Some(period(d(2025, 12, 1), d(2026, 1, 1)))
        );
        assert_eq!(
            c.literal("12-31"),
            Some(period(d(2026, 12, 31), d(2027, 1, 1)))
        );
        assert_eq!(
            c.literal("2026-10-23T14:30"),
            Some(Period {
                start: Some(date(2026, 10, 23).at(14, 30, 0, 0)),
                end: Some(date(2026, 10, 23).at(14, 31, 0, 0)),
            })
        );
        assert_eq!(
            c.literal("2026-10-23T14:30:05"),
            Some(Period {
                start: Some(date(2026, 10, 23).at(14, 30, 5, 0)),
                end: Some(date(2026, 10, 23).at(14, 30, 6, 0)),
            })
        );
        for bad in [
            "2026-02-30",
            "2026-13",
            "26-10-23",
            "2026",
            "x",
            "2026-10-23T25:00",
            "2026-10T10:00",
        ] {
            assert_eq!(c.literal(bad), None, "{bad}");
        }
    }

    #[test]
    fn instants_across_dst() {
        let c = clock();
        // 2026-03-29 has 23 hours in Berlin: clocks jump from 02:00 to 03:00.
        let start = c.instant(midnight(date(2026, 3, 29))).unwrap();
        let end = c.instant(midnight(date(2026, 3, 30))).unwrap();
        assert_eq!(start.to_string(), "2026-03-28T23:00:00Z");
        assert_eq!(end.to_string(), "2026-03-29T22:00:00Z");
        // 2026-10-25 has 25 hours.
        let start = c.instant(midnight(date(2026, 10, 25))).unwrap();
        let end = c.instant(midnight(date(2026, 10, 26))).unwrap();
        assert_eq!(end.as_second() - start.as_second(), 25 * 3600);
        // A time inside the gap moves forward.
        let gap = c.instant(date(2026, 3, 29).at(2, 30, 0, 0)).unwrap();
        assert_eq!(gap.to_string(), "2026-03-29T01:30:00Z");
    }

    #[test]
    fn conversions() {
        assert_eq!(
            date_ceil(date(2026, 1, 1).at(0, 0, 0, 0)),
            Some(date(2026, 1, 1))
        );
        assert_eq!(
            date_ceil(date(2026, 1, 1).at(0, 1, 0, 0)),
            Some(date(2026, 1, 2))
        );
        assert_eq!(
            to_time_date(date(2026, 2, 28)),
            Some(time::macros::date!(2026 - 02 - 28))
        );
        let at: Timestamp = "2026-10-22T12:05:00Z".parse().unwrap();
        assert_eq!(
            to_offset(at),
            Some(time::macros::datetime!(2026-10-22 12:05 UTC))
        );
    }
}
