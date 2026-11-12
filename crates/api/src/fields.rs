//! Custom fields and value sets (FLD): fields of the instance, value sets and their values, and the
//! fields of a project with their settings there.

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};

use crate::access::GroupRef;
use crate::accounts::UserRef;
use crate::projects::ProjectRef;

/// What a field holds (FLD-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// One line of text.
    String,
    /// Markdown.
    Text,
    Integer,
    Float,
    Date,
    DateTime,
    /// Whole minutes.
    Duration,
    /// Named values.
    Enum,
    /// Named values with a resolved flag; always one value.
    State,
    /// Accounts.
    User,
    /// Groups.
    Group,
    Version,
    Build,
    /// Named values with an owner, such as components.
    Owned,
}

/// What a value set holds (FLD-029).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SetKind {
    Enum,
    State,
    User,
    Version,
    Build,
    Owned,
}

/// A field of the default set (FLD-070); later features find it by this key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinField {
    Priority,
    Type,
    State,
    Assignee,
    Component,
    FixVersions,
    AffectedVersions,
    FixedInBuild,
    DueDate,
    Estimation,
    SpentTime,
}

/// A field as a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FieldRef {
    pub id: i64,
    pub name: String,
    pub kind: FieldKind,
    pub multi: bool,
    #[schema(required = true)]
    pub builtin: Option<BuiltinField>,
}

/// A value set as a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ValueSetRef {
    pub id: i64,
    pub name: String,
    pub kind: SetKind,
}

/// A value as a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ValueRef {
    pub id: i64,
    pub name: String,
    /// Color scheme 0–11 of the value palette (FLD-044).
    #[schema(required = true)]
    pub color: Option<i16>,
    pub archived: bool,
}

/// A field of the instance (FLD-015…FLD-025).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Field {
    pub id: i64,
    pub name: String,
    pub kind: FieldKind,
    pub multi: bool,
    /// Other words for it in searches and commands (FLD-017).
    pub aliases: Vec<String>,
    #[schema(required = true)]
    pub builtin: Option<BuiltinField>,
    /// Attached to every new blank project (FLD-020).
    pub auto_attach: bool,
    /// The set new attachments share or copy.
    #[schema(required = true)]
    pub set: Option<ValueSetRef>,
    /// What a new attachment starts with (FLD-019).
    pub can_be_empty: bool,
    pub empty_label: String,
    pub private: bool,
    pub default_values: Vec<ValueRef>,
    /// Default accounts of a user field.
    pub default_users: Vec<UserRef>,
    /// How many projects use it.
    pub project_count: i64,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Filters of the field list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct IssueFieldsQuery {
    /// Only the fields of this project.
    pub project: Option<i64>,
}

/// Fields of the projects the caller reads issues in (QRY-078): the issue list's columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueFields {
    /// Most used first (by how many of those projects have the field), then the instance order.
    pub items: Vec<FieldRef>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FieldQuery {
    /// Part of the name or an alias, in any letter case.
    pub q: Option<String>,
    /// `true`: only fields no project uses; `false`: only used ones.
    pub unused: Option<bool>,
}

/// Where a new field takes its values from (FLD-015).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum NewFieldSet {
    /// A new empty set; named after the field when no name is given.
    New { name: Option<String> },
    /// An existing set of the same kind, such as another field's.
    Existing { set_id: i64 },
}

/// A field to create (FLD-015).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewField {
    /// Up to 100 characters without `<`, `>`, `/`, `:`; unique for its type.
    pub name: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub multi: bool,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// For kinds with values; a new empty set when absent.
    pub set: Option<NewFieldSet>,
    #[serde(default = "yes")]
    pub can_be_empty: bool,
    #[serde(default)]
    pub empty_label: String,
    #[serde(default)]
    pub private: bool,
    /// Only with an existing set: values of it, or accounts for user fields.
    #[serde(default)]
    pub default_values: Vec<i64>,
    #[serde(default)]
    pub auto_attach: bool,
}

fn yes() -> bool {
    true
}

/// Changes of a field; absent fields stay. Name and aliases are shared by every project; the
/// rest is what new attachments start with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FieldPatch {
    pub name: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub auto_attach: Option<bool>,
    pub can_be_empty: Option<bool>,
    pub empty_label: Option<String>,
    pub private: Option<bool>,
    pub default_values: Option<Vec<i64>>,
    /// The version the change was made on.
    pub version: i64,
}

/// Where to move something in an order: right after another item, or first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MoveAfter {
    /// `null` moves it first.
    #[schema(required = true)]
    pub after: Option<i64>,
}

/// What deleting a field takes away (FLD-023).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FieldDeletionImpact {
    /// Projects it is removed from.
    pub projects: i64,
    /// Issues that lose a value of it.
    pub issues: i64,
    /// Boards that stop using it (BRD-022).
    pub boards: i64,
}

/// How many issues an action on a field or value touches (FLD-032, FLD-054).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueImpact {
    pub issues: i64,
}

/// Options of deleting a value (FLD-032).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeleteValueQuery {
    /// Another value of the set that issues holding the deleted one get; omit to clear it.
    pub replacement: Option<i64>,
}

/// Projects using a field or a set: those the caller may see, and how many others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UsedIn {
    pub items: Vec<ProjectRef>,
    pub hidden: i64,
}

/// A value of a set (FLD-043…FLD-049). Properties a kind does not have stay at their defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SetValue {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// Color scheme 0–11 of the value palette.
    #[schema(required = true)]
    pub color: Option<i16>,
    /// Hidden from pickers, kept on issues (FLD-045).
    pub archived: bool,
    /// State values: issues holding it count as resolved (FLD-046).
    pub resolved: bool,
    /// Version values (FLD-047).
    pub released: bool,
    #[schema(required = true)]
    pub release_date: Option<Date>,
    #[schema(required = true)]
    pub start_date: Option<Date>,
    /// Build values (FLD-048).
    #[schema(required = true)]
    pub assembled_on: Option<Date>,
    /// Values with an owner (FLD-049).
    #[schema(required = true)]
    pub owner: Option<UserRef>,
    pub version: i64,
}

/// A value set with its values or members and who uses it (FLD-029…FLD-040).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ValueSet {
    pub id: i64,
    pub name: String,
    pub kind: SetKind,
    /// In order; empty for user sets.
    pub values: Vec<SetValue>,
    /// Accounts of a user set.
    pub users: Vec<UserRef>,
    /// Groups of a user set; all their members are offered.
    pub groups: Vec<GroupRef>,
    /// Projects using it.
    pub used_in: UsedIn,
    /// Fields that offer it to new attachments.
    pub field_count: i64,
    /// The caller may change it: `system.write`, or `project.update` in every project using it.
    pub editable: bool,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// A set of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ValueSetSummary {
    pub id: i64,
    pub name: String,
    pub kind: SetKind,
    /// Projects using it.
    pub projects: i64,
    /// Fields offering it to new attachments.
    pub fields: i64,
    pub version: i64,
}

/// Filters of the set list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ValueSetQuery {
    /// Part of the name, in any letter case.
    pub q: Option<String>,
    pub kind: Option<SetKind>,
}

/// A set to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewValueSet {
    pub name: String,
    pub kind: SetKind,
}

/// A new name for a set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ValueSetPatch {
    pub name: String,
    pub version: i64,
}

/// A value to add (FLD-032). Properties of other kinds are refused.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewValue {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub color: Option<i16>,
    #[serde(default)]
    pub archived: bool,
    pub resolved: Option<bool>,
    pub released: Option<bool>,
    pub release_date: Option<Date>,
    pub start_date: Option<Date>,
    pub assembled_on: Option<Date>,
    pub owner_id: Option<i64>,
    /// Right after this value; at the end when absent.
    pub after: Option<i64>,
}

/// Changes of a value; absent fields stay, `null` clears.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ValuePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::patch::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<i16>, nullable)]
    pub color: Option<Option<i16>>,
    pub archived: Option<bool>,
    pub resolved: Option<bool>,
    pub released: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::patch::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<Date>, nullable)]
    pub release_date: Option<Option<Date>>,
    #[serde(
        default,
        deserialize_with = "crate::patch::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<Date>, nullable)]
    pub start_date: Option<Option<Date>>,
    #[serde(
        default,
        deserialize_with = "crate::patch::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<Date>, nullable)]
    pub assembled_on: Option<Option<Date>>,
    #[serde(
        default,
        deserialize_with = "crate::patch::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<i64>, nullable)]
    pub owner_id: Option<Option<i64>>,
    pub version: i64,
}

/// Accounts and groups to add to a user set or remove from it (FLD-040).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SetMembersChange {
    #[serde(default)]
    pub users: Vec<i64>,
    #[serde(default)]
    pub groups: Vec<i64>,
}

/// A field of a project with its settings there (FLD-051…FLD-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectField {
    pub field: FieldRef,
    /// Other words for it in searches and commands.
    pub aliases: Vec<String>,
    /// The set the project uses.
    #[schema(required = true)]
    pub set: Option<ValueSetRef>,
    /// Other projects using the same set: changing it changes theirs too (FLD-031).
    pub shared_with: i64,
    /// Some field offers the set to new attachments.
    pub field_default: bool,
    pub can_be_empty: bool,
    /// Shown for an empty value; usable in searches (FLD-056).
    pub empty_label: String,
    pub default_values: Vec<ValueRef>,
    pub default_users: Vec<UserRef>,
    /// Needs the private-field permissions (FLD-060).
    pub private: bool,
    /// Must be filled on new issues: cannot be empty and has no default (FLD-057).
    pub required: bool,
    pub version: i64,
}

/// The fields of a project in its order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectFields {
    pub items: Vec<ProjectField>,
}

/// How a project takes an existing field's values (FLD-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SetUse {
    /// Share the field's set with every project sharing it.
    Shared,
    /// A copy owned by the project.
    Copy,
}

/// A field to attach to a project (FLD-052, FLD-053).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum AttachField {
    Existing {
        field_id: i64,
        set: SetUse,
    },
    /// A new field whose values are the project's own set.
    New {
        name: String,
        kind: FieldKind,
        #[serde(default)]
        multi: bool,
        #[serde(default)]
        aliases: Vec<String>,
    },
}

/// Changes of a field's settings in a project; absent fields stay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectFieldPatch {
    pub can_be_empty: Option<bool>,
    pub empty_label: Option<String>,
    /// Values of the project's set, or accounts for user fields.
    pub default_values: Option<Vec<i64>>,
    pub private: Option<bool>,
    pub version: i64,
}
