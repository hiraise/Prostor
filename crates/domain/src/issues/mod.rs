//! Issues (ISS): texts and their limits, readable IDs, field values, who may do what, resolution,
//! moving between projects, tags and the records of the change history. Design: decisions.md
//! D-029.

pub mod history;
pub mod moves;
pub mod resolution;
pub mod rights;
pub mod values;

use crate::accounts::plain_text;
use crate::validation::{Problem, Rejection};

/// Longest summary, in characters.
pub const SUMMARY_MAX: usize = 500;
/// Largest description, in bytes of Markdown (256 KiB).
pub const DESCRIPTION_MAX: usize = 262_144;
/// Most tags on one issue.
pub const TAGS_PER_ISSUE_MAX: usize = 100;
/// Longest tag name.
pub const TAG_NAME_MAX: usize = 50;
/// Most tags one person owns.
pub const TAGS_PER_OWNER_MAX: usize = 1_000;
/// Most accounts and groups in an issue's audience together.
pub const AUDIENCE_MAX: usize = 50;
/// Number of tag color schemes: the same palette as values (FLD-044).
pub const TAG_COLORS: i16 = 12;

/// Validates a summary (ISS-001): one line of plain text; line breaks and tabs become spaces. A
/// draft may have none yet; a published issue must.
pub fn summary(raw: &str, required: bool) -> Result<String, Rejection> {
    let line: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let text = line.trim();
    if text.is_empty() {
        return if required {
            Err(Rejection::new(Problem::Required, "summary is required"))
        } else {
            Ok(String::new())
        };
    }
    if text.chars().count() > SUMMARY_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("summary is longer than {SUMMARY_MAX} characters"),
        ));
    }
    Ok(text.to_owned())
}

/// Validates a description (ISS-001): Markdown up to the limit; may be empty. Trailing white space
/// goes.
pub fn description(raw: &str) -> Result<String, Rejection> {
    let text = raw.trim_end();
    if text.len() > DESCRIPTION_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("description is larger than {DESCRIPTION_MAX} bytes"),
        ));
    }
    Ok(text.to_owned())
}

/// Validates a tag name (ISS-034): one line, unique per owner ignoring case (checked by the
/// store).
pub fn tag_name(raw: &str) -> Result<String, Rejection> {
    plain_text(raw, TAG_NAME_MAX, "name")
}

/// Validates a tag color scheme; `None` is no color.
pub fn tag_color(color: Option<i16>) -> Result<Option<i16>, Rejection> {
    match color {
        Some(index) if !(0..TAG_COLORS).contains(&index) => Err(Rejection::new(
            Problem::OutOfRange,
            format!("color must be from 0 to {}", TAG_COLORS - 1),
        )),
        other => Ok(other),
    }
}

/// A readable issue ID such as `WEB-42` (ISS-002): a project key, current or former, and a number
/// in that project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableId {
    pub key: String,
    pub number: i64,
}

impl ReadableId {
    /// Reads `KEY-123` in any case. Keys have no hyphen (PRJ-003), so the last one splits.
    pub fn parse(raw: &str) -> Option<Self> {
        let (key, number) = raw.trim().rsplit_once('-')?;
        let key = key.to_ascii_uppercase();
        let valid_key = key.starts_with(|c: char| c.is_ascii_uppercase())
            && key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if !valid_key || number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let number: i64 = number.parse().ok()?;
        (number >= 1).then_some(Self { key, number })
    }
}

/// Formats a readable ID.
pub fn readable_id(key: &str, number: i64) -> String {
    format!("{key}-{number}")
}

/// Checks the size of an audience (ISS-041).
pub fn check_audience(users: usize, groups: usize) -> Result<(), Rejection> {
    if users + groups > AUDIENCE_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("an audience lists at most {AUDIENCE_MAX} accounts and groups"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iss_001_summary_and_description() {
        assert_eq!(
            summary("  Crash\non start\t ", true).unwrap(),
            "Crash on start"
        );
        assert_eq!(summary("   ", true).unwrap_err().problem, Problem::Required);
        assert_eq!(summary("   ", false).unwrap(), "");
        assert!(summary(&"x".repeat(SUMMARY_MAX), true).is_ok());
        assert_eq!(
            summary(&"x".repeat(SUMMARY_MAX + 1), true)
                .unwrap_err()
                .problem,
            Problem::TooLong
        );
        assert_eq!(description("text\n\n  ").unwrap(), "text");
        assert_eq!(
            description(&"x".repeat(DESCRIPTION_MAX + 1))
                .unwrap_err()
                .problem,
            Problem::TooLong
        );
    }

    #[test]
    fn iss_002_readable_ids() {
        assert_eq!(
            ReadableId::parse(" web-42 "),
            Some(ReadableId {
                key: "WEB".to_owned(),
                number: 42
            })
        );
        assert_eq!(
            ReadableId::parse("A_1-7").map(|id| id.key),
            Some("A_1".to_owned())
        );
        for bad in [
            "WEB", "WEB-", "-4", "WEB-0", "WEB-x", "1WEB-2", "WEB-+3", "WE B-1",
        ] {
            assert_eq!(ReadableId::parse(bad), None, "{bad}");
        }
        assert_eq!(readable_id("WEB", 42), "WEB-42");
    }

    #[test]
    fn iss_034_tag_names_and_colors() {
        assert_eq!(tag_name(" later ").unwrap(), "later");
        assert!(tag_name("").is_err());
        assert!(tag_name(&"x".repeat(TAG_NAME_MAX + 1)).is_err());
        assert!(tag_color(Some(11)).is_ok());
        assert!(tag_color(Some(12)).is_err());
        assert!(check_audience(30, 20).is_ok());
        assert!(check_audience(30, 21).is_err());
    }
}
