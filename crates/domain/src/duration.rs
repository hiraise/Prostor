//! Durations (TIME-005, TIME-006, FLD-007): stored as whole minutes, entered and shown in weeks,
//! days, hours and minutes of the work schedule. The schedule applies only to input and display;
//! changing it never rewrites stored minutes (architecture.md §9.12).
//!
//! Input: whole numbers with a unit letter, in the order weeks, days, hours, minutes, each unit at
//! most once, spaces allowed: `1w 2d`, `1h30m`, `90m`. Letters are Latin (`w d h m`) or Russian
//! (`н д ч м`), in any case. The web app parses with the same rules (`web/src/lib/time/duration.ts`);
//! both run the cases in `duration_cases.json`.

use crate::settings::Language;
use crate::validation::{Problem, Rejection};

/// Longest duration: a billion minutes (about 1 900 years), exact in JavaScript too.
pub const DURATION_MAX: i64 = 1_000_000_000;

/// The work schedule durations are read and shown with. Configurable with TIME-001…TIME-003
/// (P2); until then 8-hour days and 5-day weeks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    /// Minutes in a work day.
    pub day: i64,
    /// Work days in a week.
    pub week_days: i64,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            day: 8 * 60,
            week_days: 5,
        }
    }
}

impl Schedule {
    fn week(self) -> i64 {
        self.day * self.week_days
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Unit {
    Minute,
    Hour,
    Day,
    Week,
}

impl Unit {
    fn of(letter: char) -> Option<Self> {
        match letter.to_lowercase().next()? {
            'w' | 'н' => Some(Unit::Week),
            'd' | 'д' => Some(Unit::Day),
            'h' | 'ч' => Some(Unit::Hour),
            'm' | 'м' => Some(Unit::Minute),
            _ => None,
        }
    }

    fn minutes(self, schedule: Schedule) -> i64 {
        match self {
            Unit::Minute => 1,
            Unit::Hour => 60,
            Unit::Day => schedule.day,
            Unit::Week => schedule.week(),
        }
    }
}

fn invalid() -> Rejection {
    Rejection::new(
        Problem::Invalid,
        "write a duration as numbers with units w, d, h, m in that order, such as 1d 4h",
    )
}

/// Reads a duration into minutes.
pub fn parse(raw: &str, schedule: Schedule) -> Result<i64, Rejection> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(Rejection::new(Problem::Required, "duration is required"));
    }
    let mut chars = text.chars().peekable();
    let mut total: i64 = 0;
    let mut last: Option<Unit> = None;
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            break;
        }
        let mut number: i64 = 0;
        let mut digits = 0;
        while let Some(c) = chars.next_if(char::is_ascii_digit) {
            digits += 1;
            number = number
                .checked_mul(10)
                .and_then(|n| n.checked_add(i64::from(u32::from(c) - u32::from('0'))))
                .filter(|n| *n <= DURATION_MAX)
                .ok_or_else(too_long)?;
        }
        if digits == 0 {
            return Err(invalid());
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let unit = chars.next().and_then(Unit::of).ok_or_else(invalid)?;
        if chars.peek().is_some_and(|c| c.is_alphabetic()) {
            return Err(invalid());
        }
        if last.is_some_and(|previous| unit >= previous) {
            return Err(invalid());
        }
        last = Some(unit);
        total = number
            .checked_mul(unit.minutes(schedule))
            .and_then(|minutes| total.checked_add(minutes))
            .filter(|total| *total <= DURATION_MAX)
            .ok_or_else(too_long)?;
    }
    Ok(total)
}

fn too_long() -> Rejection {
    Rejection::new(
        Problem::OutOfRange,
        format!("duration is longer than {DURATION_MAX} minutes"),
    )
}

/// Shows minutes in the largest units of the schedule: 90 → `1h 30m`, 0 → `0m`.
pub fn format(minutes: i64, schedule: Schedule, language: Language) -> String {
    let letters = match language {
        Language::En => ['w', 'd', 'h', 'm'],
        Language::Ru => ['н', 'д', 'ч', 'м'],
    };
    let mut rest = minutes.max(0);
    let mut parts = Vec::new();
    let sizes = [schedule.week(), schedule.day, 60, 1];
    for (size, letter) in sizes.into_iter().zip(letters) {
        if size <= 0 {
            continue;
        }
        let count = rest / size;
        rest %= size;
        if count > 0 {
            parts.push(format!("{count}{letter}"));
        }
    }
    if parts.is_empty() {
        return format!("0{}", letters[3]);
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct Cases {
        parse: Vec<ParseCase>,
        format: Vec<FormatCase>,
    }

    #[derive(Deserialize)]
    struct ParseCase {
        input: String,
        /// Minutes, or the problem code.
        minutes: Option<i64>,
        problem: Option<String>,
        #[serde(default)]
        day: Option<i64>,
    }

    #[derive(Deserialize)]
    struct FormatCase {
        minutes: i64,
        en: String,
        ru: String,
        #[serde(default)]
        day: Option<i64>,
    }

    fn schedule(day: Option<i64>) -> Schedule {
        Schedule {
            day: day.unwrap_or(480),
            week_days: 5,
        }
    }

    #[test]
    fn time_005_006_shared_cases() {
        let cases: Cases = serde_json::from_str(include_str!("duration_cases.json")).unwrap();
        for case in cases.parse {
            let result = parse(&case.input, schedule(case.day));
            match (case.minutes, case.problem) {
                (Some(minutes), None) => assert_eq!(result, Ok(minutes), "{:?}", case.input),
                (None, Some(problem)) => assert_eq!(
                    result.map_err(|r| r.problem.code()),
                    Err(problem.as_str()),
                    "{:?}",
                    case.input
                ),
                _ => panic!("a case needs minutes or a problem: {:?}", case.input),
            }
        }
        for case in cases.format {
            assert_eq!(
                format(case.minutes, schedule(case.day), Language::En),
                case.en
            );
            assert_eq!(
                format(case.minutes, schedule(case.day), Language::Ru),
                case.ru
            );
        }
    }
}
