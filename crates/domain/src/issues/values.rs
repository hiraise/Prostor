//! Values of custom fields on issues (FLD-001…FLD-014): what each kind of field accepts, the
//! limits of FLD-028, multi-valued changes, defaults and required fields (FLD-055…FLD-057).
//!
//! Whether a reference is allowed — a value of the project's set, an account of the user set, a
//! group the actor may see — needs the database; the service checks that after parsing here.

use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, OffsetDateTime, UtcOffset};

use crate::duration::{self, DURATION_MAX, Schedule};
use crate::fields::{
    FieldKind, FieldType, INTEGER_MAX, STRING_VALUE_MAX, TEXT_VALUE_MAX, YEAR_MAX, YEAR_MIN,
};
use crate::validation::{Problem, Rejection};

/// A value of one field on one issue. An empty field has no value at all.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    /// Markdown.
    Text(String),
    Integer(i64),
    Float(f64),
    Date(Date),
    /// In UTC.
    DateTime(OffsetDateTime),
    /// Whole minutes.
    Duration(i64),
    /// Values of a set, accounts or groups, by kind; never empty, without repeats, in the order
    /// they were given.
    Refs(Vec<i64>),
}

impl Value {
    /// The references of an enumerated field; empty for other kinds.
    pub fn refs(&self) -> &[i64] {
        match self {
            Value::Refs(ids) => ids,
            _ => &[],
        }
    }
}

/// A value as a client sends it: the field's kind says how to read it.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Integer(i64),
    Float(f64),
    Text(String),
    Ids(Vec<i64>),
}

fn invalid(message: impl Into<String>) -> Rejection {
    Rejection::new(Problem::Invalid, message)
}

fn year_in_range(year: i32) -> Result<(), Rejection> {
    if (YEAR_MIN..=YEAR_MAX).contains(&year) {
        Ok(())
    } else {
        Err(Rejection::new(
            Problem::OutOfRange,
            format!("the year must be from {YEAR_MIN} to {YEAR_MAX}"),
        ))
    }
}

/// Reads a date `YYYY-MM-DD`.
pub fn parse_date(text: &str) -> Result<Date, Rejection> {
    let date = Date::parse(text.trim(), format_description!("[year]-[month]-[day]"))
        .map_err(|_| invalid("a date is written as YYYY-MM-DD"))?;
    year_in_range(date.year())?;
    Ok(date)
}

/// Reads a date and time in RFC 3339 and keeps it in UTC.
pub fn parse_date_time(text: &str) -> Result<OffsetDateTime, Rejection> {
    let at = OffsetDateTime::parse(text.trim(), &Rfc3339)
        .map_err(|_| invalid("a date and time is written in RFC 3339"))?
        .to_offset(UtcOffset::UTC);
    year_in_range(at.year())?;
    Ok(at)
}

fn integer(value: i64) -> Result<i64, Rejection> {
    if value.abs() > INTEGER_MAX {
        return Err(Rejection::new(
            Problem::OutOfRange,
            format!("the number must be within ±{INTEGER_MAX}"),
        ));
    }
    Ok(value)
}

fn minutes(value: i64) -> Result<i64, Rejection> {
    if !(0..=DURATION_MAX).contains(&value) {
        return Err(Rejection::new(
            Problem::OutOfRange,
            format!("a duration is from 0 to {DURATION_MAX} minutes"),
        ));
    }
    Ok(value)
}

/// Reads a client's value for a field (FLD-001…FLD-014, FLD-028). `None`, an empty text or an
/// empty list clears the field. References are not checked against sets here.
pub fn parse(
    field: FieldType,
    input: Option<Input>,
    schedule: Schedule,
) -> Result<Option<Value>, Rejection> {
    let Some(input) = input else {
        return Ok(None);
    };
    if let Input::Text(text) = &input
        && text.trim().is_empty()
    {
        return Ok(None);
    }
    let value = match (field.kind, input) {
        (FieldKind::String, Input::Text(text)) => {
            let text = text.trim();
            if text.chars().count() > STRING_VALUE_MAX {
                return Err(Rejection::new(
                    Problem::TooLong,
                    format!("the text is longer than {STRING_VALUE_MAX} characters"),
                ));
            }
            if text.chars().any(char::is_control) {
                return Err(invalid("the text is one line"));
            }
            Value::String(text.to_owned())
        }
        (FieldKind::Text, Input::Text(text)) => {
            let text = text.trim_end();
            if text.len() > TEXT_VALUE_MAX {
                return Err(Rejection::new(
                    Problem::TooLong,
                    format!("the text is larger than {TEXT_VALUE_MAX} bytes"),
                ));
            }
            Value::Text(text.to_owned())
        }
        (FieldKind::Integer, Input::Integer(number)) => Value::Integer(integer(number)?),
        (FieldKind::Integer, Input::Text(text)) => Value::Integer(integer(
            text.trim()
                .parse()
                .map_err(|_| invalid("a whole number is expected"))?,
        )?),
        (FieldKind::Float, Input::Float(number)) => {
            if !number.is_finite() {
                return Err(invalid("a finite number is expected"));
            }
            Value::Float(number)
        }
        (FieldKind::Float, Input::Integer(number)) => {
            // Exact: integers are limited to 2⁵³ − 1 anyway.
            #[allow(clippy::cast_precision_loss)]
            let number = integer(number)? as f64;
            Value::Float(number)
        }
        (FieldKind::Float, Input::Text(text)) => {
            let number: f64 = text
                .trim()
                .parse()
                .map_err(|_| invalid("a number is expected"))?;
            if !number.is_finite() {
                return Err(invalid("a finite number is expected"));
            }
            Value::Float(number)
        }
        (FieldKind::Date, Input::Text(text)) => Value::Date(parse_date(&text)?),
        (FieldKind::DateTime, Input::Text(text)) => Value::DateTime(parse_date_time(&text)?),
        (FieldKind::Duration, Input::Integer(number)) => Value::Duration(minutes(number)?),
        (FieldKind::Duration, Input::Text(text)) => {
            Value::Duration(minutes(duration::parse(&text, schedule)?)?)
        }
        (
            FieldKind::Enum
            | FieldKind::State
            | FieldKind::User
            | FieldKind::Group
            | FieldKind::Version
            | FieldKind::Build
            | FieldKind::Owned,
            input,
        ) => {
            let ids = match input {
                Input::Integer(id) => vec![id],
                Input::Ids(ids) => ids,
                _ => return Err(invalid("an ID or a list of IDs is expected")),
            };
            let ids = distinct(ids);
            if ids.is_empty() {
                return Ok(None);
            }
            if !field.multi && ids.len() > 1 {
                return Err(invalid("this field holds one value"));
            }
            Value::Refs(ids)
        }
        (kind, _) => {
            return Err(invalid(format!(
                "this is not a value of a {} field",
                kind.key()
            )));
        }
    };
    Ok(Some(value))
}

fn distinct(ids: Vec<i64>) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::with_capacity(ids.len());
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Adds references to a multi-valued field (FLD-001), keeping the others.
pub fn add(current: Option<&Value>, ids: &[i64]) -> Option<Value> {
    let mut all = current.map(|v| v.refs().to_vec()).unwrap_or_default();
    all.extend_from_slice(ids);
    let all = distinct(all);
    (!all.is_empty()).then_some(Value::Refs(all))
}

/// Removes references from a multi-valued field (FLD-001), keeping the others.
pub fn remove(current: Option<&Value>, ids: &[i64]) -> Option<Value> {
    let left: Vec<i64> = current
        .map(|v| v.refs().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|id| !ids.contains(id))
        .collect();
    (!left.is_empty()).then_some(Value::Refs(left))
}

/// What happens when a field must not be empty but is (FLD-056, FLD-057).
pub fn check_not_empty(can_be_empty: bool, value: Option<&Value>) -> Result<(), Rejection> {
    if !can_be_empty && value.is_none() {
        return Err(Rejection::new(
            Problem::Required,
            "this field cannot be empty",
        ));
    }
    Ok(())
}

/// The value a new issue starts with (FLD-055): the defaults of the project's settings, if any.
pub fn default_value(defaults: &[i64]) -> Option<Value> {
    let ids = distinct(defaults.to_vec());
    (!ids.is_empty()).then_some(Value::Refs(ids))
}

/// Whether the actor may change a field by hand. Spent time is computed from work items (TIME,
/// P2), so it is read-only on issues (ISS-019).
pub fn editable(builtin: Option<&str>) -> bool {
    builtin != Some("spent_time")
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    fn single(kind: FieldKind) -> FieldType {
        FieldType::new(kind, false).unwrap()
    }

    fn read(kind: FieldKind, input: Input) -> Result<Option<Value>, Rejection> {
        parse(single(kind), Some(input), Schedule::default())
    }

    fn text(s: &str) -> Input {
        Input::Text(s.to_owned())
    }

    #[test]
    fn fld_002_003_texts() {
        assert_eq!(
            read(FieldKind::String, text("  a b ")).unwrap(),
            Some(Value::String("a b".to_owned()))
        );
        assert!(read(FieldKind::String, text("a\nb")).is_err());
        assert!(read(FieldKind::String, text(&"x".repeat(STRING_VALUE_MAX + 1))).is_err());
        assert_eq!(read(FieldKind::String, text("   ")).unwrap(), None);
        assert_eq!(
            read(FieldKind::Text, text("# a\n\nb\n")).unwrap(),
            Some(Value::Text("# a\n\nb".to_owned()))
        );
        assert!(read(FieldKind::Text, text(&"x".repeat(TEXT_VALUE_MAX + 1))).is_err());
        assert!(read(FieldKind::String, Input::Integer(1)).is_err());
    }

    #[test]
    fn fld_004_numbers() {
        assert_eq!(
            read(FieldKind::Integer, Input::Integer(-5)).unwrap(),
            Some(Value::Integer(-5))
        );
        assert_eq!(
            read(FieldKind::Integer, text(" 12 ")).unwrap(),
            Some(Value::Integer(12))
        );
        assert!(read(FieldKind::Integer, Input::Integer(INTEGER_MAX + 1)).is_err());
        assert!(read(FieldKind::Integer, Input::Float(1.5)).is_err());
        assert_eq!(
            read(FieldKind::Float, Input::Integer(2)).unwrap(),
            Some(Value::Float(2.0))
        );
        assert_eq!(
            read(FieldKind::Float, text("2.5")).unwrap(),
            Some(Value::Float(2.5))
        );
        assert!(read(FieldKind::Float, Input::Float(f64::NAN)).is_err());
        assert!(read(FieldKind::Float, text("inf")).is_err());
    }

    #[test]
    fn fld_005_006_dates() {
        assert_eq!(
            read(FieldKind::Date, text("2026-10-22")).unwrap(),
            Some(Value::Date(date!(2026 - 10 - 22)))
        );
        assert!(read(FieldKind::Date, text("22.10.2026")).is_err());
        assert!(read(FieldKind::Date, text("0000-01-01")).is_err());
        assert_eq!(
            read(FieldKind::DateTime, text("2026-10-22T15:00:00+03:00")).unwrap(),
            Some(Value::DateTime(datetime!(2026-10-22 12:00 UTC)))
        );
        assert!(read(FieldKind::DateTime, text("2026-10-22")).is_err());
    }

    #[test]
    fn fld_007_durations() {
        assert_eq!(
            read(FieldKind::Duration, text("1h 30m")).unwrap(),
            Some(Value::Duration(90))
        );
        assert_eq!(
            read(FieldKind::Duration, Input::Integer(45)).unwrap(),
            Some(Value::Duration(45))
        );
        assert!(read(FieldKind::Duration, Input::Integer(-1)).is_err());
        assert!(read(FieldKind::Duration, Input::Integer(DURATION_MAX + 1)).is_err());
    }

    #[test]
    fn fld_001_references_single_and_multi() {
        assert_eq!(
            read(FieldKind::Enum, Input::Integer(7)).unwrap(),
            Some(Value::Refs(vec![7]))
        );
        assert!(read(FieldKind::State, Input::Ids(vec![1, 2])).is_err());
        assert_eq!(read(FieldKind::User, Input::Ids(vec![])).unwrap(), None);
        let multi = FieldType::new(FieldKind::Version, true).unwrap();
        assert_eq!(
            parse(multi, Some(Input::Ids(vec![3, 1, 3])), Schedule::default()).unwrap(),
            Some(Value::Refs(vec![3, 1]))
        );
        assert!(read(FieldKind::Group, text("x")).is_err());
        assert_eq!(
            parse(single(FieldKind::Enum), None, Schedule::default()).unwrap(),
            None
        );
        let current = Value::Refs(vec![1, 2]);
        assert_eq!(
            add(Some(&current), &[2, 3]),
            Some(Value::Refs(vec![1, 2, 3]))
        );
        assert_eq!(add(None, &[4]), Some(Value::Refs(vec![4])));
        assert_eq!(remove(Some(&current), &[1]), Some(Value::Refs(vec![2])));
        assert_eq!(remove(Some(&current), &[1, 2]), None);
    }

    #[test]
    fn fld_055_057_defaults_and_empty_fields() {
        assert_eq!(default_value(&[]), None);
        assert_eq!(default_value(&[5, 5]), Some(Value::Refs(vec![5])));
        assert!(check_not_empty(false, None).is_err());
        assert!(check_not_empty(false, Some(&Value::Integer(1))).is_ok());
        assert!(check_not_empty(true, None).is_ok());
        assert!(!editable(Some("spent_time")));
        assert!(editable(Some("estimation")));
        assert!(editable(None));
    }
}
