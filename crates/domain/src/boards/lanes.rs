//! Lanes (BRD-049…BRD-057, BRD-096; plan 1.8 §3.14): the rows of a board, by the values of a
//! field or attribute, or by parent issues. A lane is named by a key, also kept in the person's
//! collapsed lanes.

use std::fmt;

use time::{Date, Duration};

use crate::fields::FieldKind;
use crate::validation::{Problem, Rejection};
use crate::{IssueId, ValueId};

/// Lanes a board page shows (BRD-057).
pub const PAGE: usize = 50;
/// Lane values a board keeps.
pub const VALUES_MAX: usize = 200;
/// Lane title length (a new parent lane's summary).
pub const TITLE_MAX: usize = 300;

macro_rules! keyed {
    ($name:ident { $($variant:ident = $key:literal),+ $(,)? }) => {
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn key(self) -> &'static str {
                match self {
                    $($name::$variant => $key),+
                }
            }

            pub fn parse(key: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|v| v.key() == key)
            }
        }
    };
}

/// What makes the lanes (BRD-049).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaneMode {
    None,
    Values,
    Parents,
}
keyed!(LaneMode { None = "none", Values = "values", Parents = "parents" });

/// An issue attribute that makes value lanes (BRD-050).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attribute {
    Project,
    Tags,
    Created,
    Updated,
}
keyed!(Attribute { Project = "project", Tags = "tags", Created = "created", Updated = "updated" });

/// Where the lane of cards matching no lane goes (BRD-052).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OtherLane {
    Off,
    Top,
    Bottom,
}
keyed!(OtherLane { Off = "off", Top = "top", Bottom = "bottom" });

/// Relative date ranges of date lanes, in the instance time zone; a week starts on Monday.
/// `Later` holds dates after today (a due date), `None` an empty date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Later,
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    ThisMonth,
    Older,
    None,
}
keyed!(Bucket {
    Later = "later",
    Today = "today",
    Yesterday = "yesterday",
    ThisWeek = "this_week",
    LastWeek = "last_week",
    ThisMonth = "this_month",
    Older = "older",
    None = "none",
});

/// The days where the date ranges start, for one `today`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub today: Date,
    pub yesterday: Date,
    pub week: Date,
    pub last_week: Date,
    pub month: Date,
}

impl Bounds {
    pub fn of(today: Date) -> Self {
        let week = today - Duration::days(i64::from(today.weekday().number_days_from_monday()));
        Self {
            today,
            yesterday: today - Duration::days(1),
            week,
            last_week: week - Duration::days(7),
            month: today.replace_day(1).unwrap_or(today),
        }
    }

    /// The range of a date: the first that holds it, in the order of [`Bucket`].
    pub fn bucket(&self, day: Option<Date>) -> Bucket {
        match day {
            None => Bucket::None,
            Some(d) if d > self.today => Bucket::Later,
            Some(d) if d == self.today => Bucket::Today,
            Some(d) if d == self.yesterday => Bucket::Yesterday,
            Some(d) if d >= self.week => Bucket::ThisWeek,
            Some(d) if d >= self.last_week => Bucket::LastWeek,
            Some(d) if d >= self.month => Bucket::ThisMonth,
            Some(_) => Bucket::Older,
        }
    }
}

/// One lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaneKey {
    /// A value of a set field (state, enum, owned, version, build).
    Value(ValueId),
    User(i64),
    Group(i64),
    Project(i64),
    Tag(i64),
    Date(Bucket),
    /// A parent issue.
    Issue(IssueId),
    /// Cards matching no lane.
    Other,
}

impl LaneKey {
    pub fn parse(key: &str) -> Option<Self> {
        if key == "other" {
            return Some(LaneKey::Other);
        }
        let (prefix, rest) = key.split_once(':')?;
        if prefix == "d" {
            return Bucket::parse(rest).map(LaneKey::Date);
        }
        // Digits only: no sign, no spaces, so a key has one spelling.
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let id: i64 = rest.parse().ok().filter(|id| *id > 0)?;
        Some(match prefix {
            "v" => LaneKey::Value(id),
            "u" => LaneKey::User(id),
            "g" => LaneKey::Group(id),
            "p" => LaneKey::Project(id),
            "t" => LaneKey::Tag(id),
            "i" => LaneKey::Issue(id),
            _ => return None,
        })
    }

    /// The ID behind a value key.
    pub fn id(self) -> Option<i64> {
        match self {
            LaneKey::Value(id)
            | LaneKey::User(id)
            | LaneKey::Group(id)
            | LaneKey::Project(id)
            | LaneKey::Tag(id)
            | LaneKey::Issue(id) => Some(id),
            LaneKey::Date(_) | LaneKey::Other => None,
        }
    }
}

impl fmt::Display for LaneKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaneKey::Value(id) => write!(f, "v:{id}"),
            LaneKey::User(id) => write!(f, "u:{id}"),
            LaneKey::Group(id) => write!(f, "g:{id}"),
            LaneKey::Project(id) => write!(f, "p:{id}"),
            LaneKey::Tag(id) => write!(f, "t:{id}"),
            LaneKey::Date(b) => write!(f, "d:{}", b.key()),
            LaneKey::Issue(id) => write!(f, "i:{id}"),
            LaneKey::Other => f.write_str("other"),
        }
    }
}

/// What the lanes of a `values` board come from, and the prefix of their keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A set field: `v:`.
    Values,
    Users,
    Groups,
    Projects,
    Tags,
    /// A date field or attribute: `d:`.
    Dates,
}

impl Source {
    /// The source of a field's value lanes (BRD-050): string, text, number and duration fields
    /// make no lanes.
    pub fn of_field(kind: FieldKind) -> Result<Self, Rejection> {
        match kind {
            FieldKind::State
            | FieldKind::Enum
            | FieldKind::Owned
            | FieldKind::Version
            | FieldKind::Build => Ok(Source::Values),
            FieldKind::User => Ok(Source::Users),
            FieldKind::Group => Ok(Source::Groups),
            FieldKind::Date | FieldKind::DateTime => Ok(Source::Dates),
            _ => Err(Rejection::new(
                Problem::Invalid,
                "lanes come from a field with a list of values, a user, group or date field",
            )),
        }
    }

    pub fn of_attribute(attribute: Attribute) -> Self {
        match attribute {
            Attribute::Project => Source::Projects,
            Attribute::Tags => Source::Tags,
            Attribute::Created | Attribute::Updated => Source::Dates,
        }
    }

    /// The SQL prefix of the keys (`d` keys are built by bucket).
    pub fn prefix(self) -> &'static str {
        match self {
            Source::Values => "v:",
            Source::Users => "u:",
            Source::Groups => "g:",
            Source::Projects => "p:",
            Source::Tags => "t:",
            Source::Dates => "d:",
        }
    }

    fn holds(self, key: LaneKey) -> bool {
        matches!(
            (self, key),
            (Source::Values, LaneKey::Value(_))
                | (Source::Users, LaneKey::User(_))
                | (Source::Groups, LaneKey::Group(_))
                | (Source::Projects, LaneKey::Project(_))
                | (Source::Tags, LaneKey::Tag(_))
                | (Source::Dates, LaneKey::Date(_))
        )
    }

    /// A card dropped into a lane of this source changes the issue (BRD-050); project and date
    /// lanes only show where a card stands.
    pub fn settable(self) -> bool {
        matches!(
            self,
            Source::Values | Source::Users | Source::Groups | Source::Tags
        )
    }
}

/// Validates the field limiting parent lanes (BRD-051): a field with a list of values; user and
/// simple fields are refused.
pub fn parent_field_allowed(kind: FieldKind) -> Result<(), Rejection> {
    match kind {
        FieldKind::State
        | FieldKind::Enum
        | FieldKind::Owned
        | FieldKind::Version
        | FieldKind::Build => Ok(()),
        _ => Err(Rejection::new(
            Problem::Invalid,
            "parent lanes are limited by a field with a list of values",
        )),
    }
}

/// The lane keys of the settings: of the source, without duplicates, at most [`VALUES_MAX`].
/// Date lanes with no keys show every range.
pub fn values(source: Source, keys: &[String]) -> Result<Vec<LaneKey>, Rejection> {
    let mut out: Vec<LaneKey> = Vec::with_capacity(keys.len());
    for raw in keys {
        let key = LaneKey::parse(raw)
            .filter(|k| source.holds(*k))
            .ok_or_else(|| Rejection::new(Problem::Invalid, format!("not a lane here: {raw}")))?;
        if !out.contains(&key) {
            out.push(key);
        }
    }
    if out.len() > VALUES_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("at most {VALUES_MAX} lanes"),
        ));
    }
    if out.is_empty() && source == Source::Dates {
        out = Bucket::ALL.iter().map(|b| LaneKey::Date(*b)).collect();
    }
    Ok(out)
}

/// The lanes in the order shown: the given ones with the other lane at its place.
pub fn ordered(lanes: Vec<LaneKey>, other: OtherLane) -> Vec<LaneKey> {
    match other {
        OtherLane::Off => lanes,
        OtherLane::Top => std::iter::once(LaneKey::Other).chain(lanes).collect(),
        OtherLane::Bottom => lanes.into_iter().chain([LaneKey::Other]).collect(),
    }
}

/// Validates a new parent lane's title (BRD-054): one line.
pub fn title(raw: &str) -> Result<String, Rejection> {
    crate::accounts::plain_text(raw, TITLE_MAX, "title")
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn brd_049_keys_and_modes() {
        for raw in [
            "v:12", "u:3", "g:4", "p:5", "t:6", "i:7", "d:today", "d:none", "other",
        ] {
            assert_eq!(LaneKey::parse(raw).unwrap().to_string(), raw);
        }
        for bad in ["v:", "v:-1", "v:0", "v:+1", "x:1", "d:soon", "v:1 ", "12"] {
            assert_eq!(LaneKey::parse(bad), None, "{bad}");
        }
        for m in LaneMode::ALL {
            assert_eq!(LaneMode::parse(m.key()), Some(*m));
        }
        for a in Attribute::ALL {
            assert_eq!(Attribute::parse(a.key()), Some(*a));
        }
        for o in OtherLane::ALL {
            assert_eq!(OtherLane::parse(o.key()), Some(*o));
        }
        assert_eq!(
            values(Source::Values, &["v:2".into(), "v:1".into(), "v:2".into()]).unwrap(),
            vec![LaneKey::Value(2), LaneKey::Value(1)]
        );
        assert!(values(Source::Values, &["u:2".into()]).is_err());
        assert_eq!(values(Source::Dates, &[]).unwrap().len(), Bucket::ALL.len());
        let many: Vec<String> = (1..=VALUES_MAX as i64 + 1)
            .map(|i| format!("t:{i}"))
            .collect();
        assert!(values(Source::Tags, &many).is_err());
    }

    #[test]
    fn brd_050_value_sources() {
        for kind in [
            FieldKind::State,
            FieldKind::Enum,
            FieldKind::Owned,
            FieldKind::Version,
            FieldKind::Build,
            FieldKind::User,
            FieldKind::Group,
            FieldKind::Date,
            FieldKind::DateTime,
        ] {
            assert!(Source::of_field(kind).is_ok(), "{kind:?}");
        }
        for kind in [
            FieldKind::String,
            FieldKind::Text,
            FieldKind::Integer,
            FieldKind::Float,
            FieldKind::Duration,
        ] {
            assert!(Source::of_field(kind).is_err(), "{kind:?}");
        }
        assert!(Source::Values.settable() && Source::Tags.settable());
        assert!(!Source::Projects.settable() && !Source::Dates.settable());
    }

    #[test]
    fn brd_050_date_ranges() {
        // Wednesday 2026-10-21.
        let b = Bounds::of(date!(2026 - 10 - 21));
        assert_eq!(b.week, date!(2026 - 10 - 19));
        assert_eq!(b.last_week, date!(2026 - 10 - 12));
        assert_eq!(b.month, date!(2026 - 10 - 01));
        assert_eq!(b.bucket(Some(date!(2026 - 10 - 22))), Bucket::Later);
        assert_eq!(b.bucket(Some(date!(2026 - 10 - 21))), Bucket::Today);
        assert_eq!(b.bucket(Some(date!(2026 - 10 - 20))), Bucket::Yesterday);
        assert_eq!(b.bucket(Some(date!(2026 - 10 - 19))), Bucket::ThisWeek);
        assert_eq!(b.bucket(Some(date!(2026 - 10 - 12))), Bucket::LastWeek);
        assert_eq!(b.bucket(Some(date!(2026 - 10 - 02))), Bucket::ThisMonth);
        assert_eq!(b.bucket(Some(date!(2026 - 09 - 30))), Bucket::Older);
        assert_eq!(b.bucket(None), Bucket::None);
    }

    #[test]
    fn brd_051_parent_fields() {
        assert!(parent_field_allowed(FieldKind::Enum).is_ok());
        assert!(parent_field_allowed(FieldKind::User).is_err());
        assert!(parent_field_allowed(FieldKind::String).is_err());
    }

    #[test]
    fn brd_052_other_lane_place() {
        let lanes = vec![LaneKey::Value(1)];
        assert_eq!(ordered(lanes.clone(), OtherLane::Off), lanes);
        assert_eq!(
            ordered(lanes.clone(), OtherLane::Top),
            vec![LaneKey::Other, LaneKey::Value(1)]
        );
        assert_eq!(
            ordered(lanes, OtherLane::Bottom),
            vec![LaneKey::Value(1), LaneKey::Other]
        );
    }
}
