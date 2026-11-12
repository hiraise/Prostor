//! Custom fields (FLD): types, names, aliases, value sets and their values, per-project settings,
//! limits, and who may change what. Design: decisions.md D-028.
//!
//! Three layers (architecture.md §9.4): a field is defined once for the instance (name, type,
//! aliases); a project attaches it with its own settings (default, empty label, privacy, order)
//! and a value set; value sets are named lists that projects share or own.

pub mod builtin;
pub mod templates;

use crate::access::Access;
use crate::access::permission::Permission;
use crate::access::{Denied, Projects};
use crate::accounts::plain_text;
use crate::validation::{Problem, Rejection};
use crate::{ProjectId, UserId};

/// Longest field name.
pub const FIELD_NAME_MAX: usize = 100;
/// Most aliases per field, and the longest one.
pub const ALIASES_MAX: usize = 10;
pub const ALIAS_MAX: usize = 50;
/// Longest label of an empty value.
pub const EMPTY_LABEL_MAX: usize = 50;
/// Longest value set name.
pub const SET_NAME_MAX: usize = 100;
/// Longest value name and description.
pub const VALUE_NAME_MAX: usize = 100;
pub const VALUE_DESCRIPTION_MAX: usize = 1_000;
/// Most values in a set (FLD-028): pickers and orders stay usable.
pub const VALUES_MAX: usize = 2_000;
/// Most accounts and groups listed in a user set.
pub const SET_USERS_MAX: usize = 2_000;
pub const SET_GROUPS_MAX: usize = 100;
/// Most fields attached to one project.
pub const PROJECT_FIELDS_MAX: usize = 100;
/// Number of value color schemes (FLD-044); the web app's palette has as many.
pub const VALUE_COLORS: i16 = 12;
/// Limits of values on issues (FLD-028), enforced when issues store them (step 1.3).
pub const STRING_VALUE_MAX: usize = 1_000;
pub const TEXT_VALUE_MAX: usize = 65_536;
/// Integers are exact in JavaScript up to 2⁵³ − 1.
pub const INTEGER_MAX: i64 = 9_007_199_254_740_991;
/// Dates and times stay within years 1–9999.
pub const YEAR_MIN: i32 = 1;
pub const YEAR_MAX: i32 = 9_999;

/// What a field holds (FLD-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldKind {
    /// One line of text.
    String,
    /// Markdown, shown below the description.
    Text,
    Integer,
    Float,
    Date,
    DateTime,
    /// Whole minutes (TIME-005).
    Duration,
    /// Named values from a set.
    Enum,
    /// Named values with a resolved flag; always single.
    State,
    /// Accounts from a user set.
    User,
    /// Any group the viewer may see; no set.
    Group,
    /// Versions with release dates.
    Version,
    /// Builds with assembly dates.
    Build,
    /// Named values with an owner, such as components.
    Owned,
}

impl FieldKind {
    pub const ALL: [FieldKind; 14] = [
        FieldKind::String,
        FieldKind::Text,
        FieldKind::Integer,
        FieldKind::Float,
        FieldKind::Date,
        FieldKind::DateTime,
        FieldKind::Duration,
        FieldKind::Enum,
        FieldKind::State,
        FieldKind::User,
        FieldKind::Group,
        FieldKind::Version,
        FieldKind::Build,
        FieldKind::Owned,
    ];

    pub fn key(self) -> &'static str {
        match self {
            FieldKind::String => "string",
            FieldKind::Text => "text",
            FieldKind::Integer => "integer",
            FieldKind::Float => "float",
            FieldKind::Date => "date",
            FieldKind::DateTime => "date_time",
            FieldKind::Duration => "duration",
            FieldKind::Enum => "enum",
            FieldKind::State => "state",
            FieldKind::User => "user",
            FieldKind::Group => "group",
            FieldKind::Version => "version",
            FieldKind::Build => "build",
            FieldKind::Owned => "owned",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// The kind of value set the field takes values from; `None` for simple kinds and groups.
    pub fn set_kind(self) -> Option<SetKind> {
        match self {
            FieldKind::Enum => Some(SetKind::Enum),
            FieldKind::State => Some(SetKind::State),
            FieldKind::User => Some(SetKind::User),
            FieldKind::Version => Some(SetKind::Version),
            FieldKind::Build => Some(SetKind::Build),
            FieldKind::Owned => Some(SetKind::Owned),
            _ => None,
        }
    }

    /// May hold several values (FLD-001): enumerated kinds except state.
    pub fn may_be_multi(self) -> bool {
        matches!(
            self,
            FieldKind::Enum
                | FieldKind::User
                | FieldKind::Group
                | FieldKind::Version
                | FieldKind::Build
                | FieldKind::Owned
        )
    }

    /// Takes default values (FLD-055): kinds with a value set.
    pub fn has_defaults(self) -> bool {
        self.set_kind().is_some()
    }
}

/// What a value set holds (FLD-029).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SetKind {
    Enum,
    State,
    User,
    Version,
    Build,
    Owned,
}

impl SetKind {
    pub const ALL: [SetKind; 6] = [
        SetKind::Enum,
        SetKind::State,
        SetKind::User,
        SetKind::Version,
        SetKind::Build,
        SetKind::Owned,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SetKind::Enum => "enum",
            SetKind::State => "state",
            SetKind::User => "user",
            SetKind::Version => "version",
            SetKind::Build => "build",
            SetKind::Owned => "owned",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// Values have a manual order (FLD-037); user sets are sorted by name (FLD-039).
    pub fn ordered(self) -> bool {
        self != SetKind::User
    }
}

/// A field's type: kind and multiplicity, which together are part of its identity (FLD-001,
/// FLD-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldType {
    pub kind: FieldKind,
    pub multi: bool,
}

impl FieldType {
    pub fn new(kind: FieldKind, multi: bool) -> Result<Self, Rejection> {
        if multi && !kind.may_be_multi() {
            return Err(Rejection::new(
                Problem::Invalid,
                format!("a {} field holds one value", kind.key()),
            ));
        }
        Ok(Self { kind, multi })
    }
}

/// Validates a field name (FLD-016): one line without `<`, `>`, `/` or `:`, which would break
/// references in searches and scripts.
pub fn field_name(raw: &str) -> Result<String, Rejection> {
    let name = plain_text(raw, FIELD_NAME_MAX, "name")?;
    if name.contains(['<', '>', '/', ':']) {
        return Err(Rejection::new(
            Problem::Invalid,
            "name cannot contain <, >, / or :",
        ));
    }
    Ok(name)
}

/// Validates aliases (FLD-017): each one line without the characters names forbid, distinct
/// ignoring case and different from the name. Empty entries are dropped.
pub fn aliases(raw: &[String], name: &str) -> Result<Vec<String>, Rejection> {
    let mut out: Vec<String> = Vec::new();
    for alias in raw {
        if alias.trim().is_empty() {
            continue;
        }
        let alias = plain_text(alias, ALIAS_MAX, "alias")?;
        if alias.contains(['<', '>', '/', ':']) {
            return Err(Rejection::new(
                Problem::Invalid,
                "an alias cannot contain <, >, / or :",
            ));
        }
        let lower = alias.to_lowercase();
        if lower == name.to_lowercase() || out.iter().any(|a| a.to_lowercase() == lower) {
            continue;
        }
        out.push(alias);
    }
    if out.len() > ALIASES_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("a field has at most {ALIASES_MAX} aliases"),
        ));
    }
    Ok(out)
}

/// Validates the label of an empty value (FLD-056): may be empty.
pub fn empty_label(raw: &str) -> Result<String, Rejection> {
    if raw.trim().is_empty() {
        return Ok(String::new());
    }
    plain_text(raw, EMPTY_LABEL_MAX, "empty label")
}

pub fn set_name(raw: &str) -> Result<String, Rejection> {
    plain_text(raw, SET_NAME_MAX, "name")
}

/// Validates a value name (FLD-043): one line, unique ignoring case within its set (checked by the
/// store).
pub fn value_name(raw: &str) -> Result<String, Rejection> {
    plain_text(raw, VALUE_NAME_MAX, "name")
}

pub fn value_description(raw: &str) -> Result<String, Rejection> {
    let text = raw.trim();
    if text.chars().count() > VALUE_DESCRIPTION_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("description is longer than {VALUE_DESCRIPTION_MAX} characters"),
        ));
    }
    Ok(text.to_owned())
}

/// Validates a color scheme index (FLD-044); `None` is no color.
pub fn value_color(color: Option<i16>) -> Result<Option<i16>, Rejection> {
    match color {
        Some(index) if !(0..VALUE_COLORS).contains(&index) => Err(Rejection::new(
            Problem::OutOfRange,
            format!("color must be from 0 to {}", VALUE_COLORS - 1),
        )),
        other => Ok(other),
    }
}

/// Checks the number of default values (FLD-055): only kinds with sets take them, a single field
/// at most one.
pub fn check_defaults(field: FieldType, count: usize) -> Result<(), Rejection> {
    if count == 0 {
        return Ok(());
    }
    if !field.kind.has_defaults() {
        return Err(Rejection::new(
            Problem::Invalid,
            "this kind of field has no default values",
        ));
    }
    if !field.multi && count > 1 {
        return Err(Rejection::new(
            Problem::Invalid,
            "a single-value field has at most one default value",
        ));
    }
    Ok(())
}

/// A field must be filled on new issues when it cannot be empty and nothing fills it by default
/// (FLD-057).
pub fn required(can_be_empty: bool, has_defaults: bool) -> bool {
    !can_be_empty && !has_defaults
}

/// Checks the label of an empty value against the empty setting (FLD-056): a field that cannot be
/// empty has no label.
pub fn check_empty_label(can_be_empty: bool, label: &str) -> Result<(), Rejection> {
    if !can_be_empty && !label.is_empty() {
        return Err(Rejection::new(
            Problem::Invalid,
            "a field that cannot be empty has no empty label",
        ));
    }
    Ok(())
}

/// Whether someone may see the instance's fields and value sets (FLD-025, FLD-027): readers of
/// the system settings, and people who configure any project (they attach existing fields).
pub fn may_see_fields(access: &Access) -> Result<(), Denied> {
    if access.has(Permission::SystemRead) || access.has_anywhere(Permission::ProjectUpdate) {
        Ok(())
    } else {
        Err(Denied::of(Permission::SystemRead))
    }
}

/// Whether someone may change the base properties of a field (FLD-018): name, aliases and the
/// settings new attachments start with. `system.write` always; otherwise only for a field that is
/// not auto-attached and is used only in projects the person configures.
pub fn may_edit_base(
    access: &Access,
    auto_attach: bool,
    used_in: &[ProjectId],
) -> Result<(), Denied> {
    if access.has(Permission::SystemWrite) {
        return Ok(());
    }
    if !auto_attach
        && !used_in.is_empty()
        && used_in
            .iter()
            .all(|project| access.has_in(Permission::ProjectUpdate, *project))
    {
        return Ok(());
    }
    Err(Denied::of(Permission::SystemWrite))
}

/// Whether someone may change a value set (FLD-031): `system.write`, or `project.update` in every
/// project using it. A set no project uses belongs to the instance settings.
pub fn may_edit_set(access: &Access, used_in: &[ProjectId]) -> Result<(), Denied> {
    may_edit_base(access, false, used_in)
}

/// Which projects of `used_in` a person may not configure: the editor names them (FLD-031).
pub fn not_configurable(access: &Access, used_in: &[ProjectId]) -> Vec<ProjectId> {
    match access.projects_with(Permission::ProjectUpdate) {
        Projects::All => Vec::new(),
        Projects::Only(allowed) => used_in
            .iter()
            .filter(|project| !allowed.contains(project))
            .copied()
            .collect(),
    }
}

/// A default value: a value of the set, or an account for user fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultValue {
    Value(i64),
    User(UserId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::capability::CapabilitySet;
    use crate::access::role::PredefinedRole;
    use crate::access::{Grant, Scope};

    #[test]
    fn fld_001_types_and_multiplicity() {
        assert!(FieldType::new(FieldKind::Enum, true).is_ok());
        assert!(FieldType::new(FieldKind::User, true).is_ok());
        assert!(FieldType::new(FieldKind::State, true).is_err());
        assert!(FieldType::new(FieldKind::Integer, true).is_err());
        assert!(FieldType::new(FieldKind::Integer, false).is_ok());
        for kind in FieldKind::ALL {
            assert_eq!(FieldKind::parse(kind.key()), Some(kind));
        }
        assert_eq!(FieldKind::Group.set_kind(), None);
        assert_eq!(FieldKind::Owned.set_kind(), Some(SetKind::Owned));
        assert!(!SetKind::User.ordered());
    }

    #[test]
    fn fld_016_017_names_and_aliases() {
        assert_eq!(field_name(" Priority ").unwrap(), "Priority");
        assert_eq!(field_name("a/b").unwrap_err().problem, Problem::Invalid);
        assert_eq!(field_name("a:b").unwrap_err().problem, Problem::Invalid);
        assert_eq!(field_name("").unwrap_err().problem, Problem::Required);
        let list = aliases(
            &[
                "for".to_owned(),
                " ".to_owned(),
                "FOR".to_owned(),
                "priority".to_owned(),
                "assigned to".to_owned(),
            ],
            "Priority",
        )
        .unwrap();
        assert_eq!(list, ["for", "assigned to"]);
        let many: Vec<String> = (0..11).map(|i| format!("a{i}")).collect();
        assert_eq!(aliases(&many, "x").unwrap_err().problem, Problem::TooLong);
    }

    #[test]
    fn fld_028_limits() {
        assert_eq!(value_color(Some(3)), Ok(Some(3)));
        assert_eq!(value_color(None), Ok(None));
        assert!(value_color(Some(VALUE_COLORS)).is_err());
        assert!(value_color(Some(-1)).is_err());
        assert!(value_name(&"x".repeat(VALUE_NAME_MAX + 1)).is_err());
        assert!(value_description(&"x".repeat(VALUE_DESCRIPTION_MAX)).is_ok());
        assert_eq!(empty_label("  ").unwrap(), "");
    }

    #[test]
    fn fld_055_057_defaults_and_required() {
        let single = FieldType::new(FieldKind::Enum, false).unwrap();
        let multi = FieldType::new(FieldKind::Version, true).unwrap();
        let simple = FieldType::new(FieldKind::Date, false).unwrap();
        assert!(check_defaults(single, 1).is_ok());
        assert!(check_defaults(single, 2).is_err());
        assert!(check_defaults(multi, 3).is_ok());
        assert!(check_defaults(simple, 0).is_ok());
        assert!(check_defaults(simple, 1).is_err());
        assert!(required(false, false));
        assert!(!required(false, true));
        assert!(!required(true, false));
        assert!(check_empty_label(false, "None").is_err());
        assert!(check_empty_label(true, "None").is_ok());
    }

    fn access(grants: Vec<Grant>) -> Access {
        Access::compile(1, grants, Vec::new(), CapabilitySet::default())
    }

    #[test]
    fn fld_018_027_031_who_changes_fields_and_sets() {
        let admin = access(vec![Grant {
            permissions: PredefinedRole::SystemAdmin.permissions(),
            scope: Scope::Global,
        }]);
        let project_admin = access(vec![
            Grant {
                permissions: PredefinedRole::ProjectAdmin.permissions(),
                scope: Scope::Project(1),
            },
            Grant {
                permissions: PredefinedRole::ProjectAdmin.permissions(),
                scope: Scope::Project(2),
            },
        ]);
        let member = access(vec![Grant {
            permissions: PredefinedRole::ProjectMember.permissions(),
            scope: Scope::Project(1),
        }]);
        assert!(may_edit_base(&admin, true, &[]).is_ok());
        assert!(may_edit_base(&project_admin, false, &[1, 2]).is_ok());
        assert!(may_edit_base(&project_admin, false, &[1, 3]).is_err());
        assert!(may_edit_base(&project_admin, true, &[1]).is_err());
        assert!(may_edit_base(&project_admin, false, &[]).is_err());
        assert!(may_edit_set(&project_admin, &[2]).is_ok());
        assert_eq!(not_configurable(&project_admin, &[1, 3, 4]), [3, 4]);
        assert!(not_configurable(&admin, &[1, 3]).is_empty());
        assert!(may_see_fields(&project_admin).is_ok());
        assert!(may_see_fields(&member).is_err());
    }
}
