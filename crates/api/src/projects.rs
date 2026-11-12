//! Projects (PRJ): the list, creating from a template, settings, favorites, archive, deletion,
//! ownership and the people of a project.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::access::{Assignment, GroupRef};
use crate::accounts::UserRef;
use crate::error::FieldError;

/// A built-in project template (PRJ-021…PRJ-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateKey {
    /// The fields marked for new projects.
    Blank,
    /// Sprints, stories and tasks.
    Scrum,
    /// A flow of stages.
    Kanban,
}

/// What a project is for (PRJ-001). Service desks come with the helpdesk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    Standard,
    ServiceDesk,
}

/// A project as a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectRef {
    pub id: i64,
    pub key: String,
    pub name: String,
    /// Background of the generated logo: 0–7.
    pub logo_color: i16,
    /// The uploaded logo (PRJ-007); `null` shows the generated one.
    #[schema(required = true)]
    pub logo_url: Option<String>,
    pub archived: bool,
}

/// Where the background deletion of a project is (PRJ-065).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeletionState {
    Queued,
    Running,
    /// Stopped with an error; deleting again starts it anew.
    Failed,
}

/// A deletion in progress, shown to those who may delete the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Deletion {
    pub state: DeletionState,
    /// What went wrong, for a failed deletion.
    #[schema(required = true)]
    pub error: Option<String>,
}

/// A project (PRJ-001…PRJ-014).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Project {
    pub id: i64,
    /// Prefix of issue IDs.
    pub key: String,
    pub name: String,
    /// Markdown.
    pub description: String,
    /// The description rendered in the basic profile (PRJ-006, D-031): no markers.
    pub description_html: String,
    pub kind: ProjectKind,
    pub template: TemplateKey,
    #[schema(required = true)]
    pub owner: Option<UserRef>,
    /// Background of the generated logo: 0–7.
    pub logo_color: i16,
    /// The uploaded logo (PRJ-007); `null` shows the generated one.
    #[schema(required = true)]
    pub logo_url: Option<String>,
    /// The number the next issue gets.
    pub next_number: i64,
    #[schema(required = true)]
    pub team: Option<GroupRef>,
    /// Accounts in the team, with the members of its groups.
    pub team_size: i64,
    /// Keys the project had before, oldest first; they still lead to it (PRJ-005).
    pub former_keys: Vec<String>,
    /// Group or team pre-filled as the audience of new issues and articles (PRJ-039).
    #[schema(required = true)]
    pub default_audience: Option<GroupRef>,
    /// Starred by the caller (PRJ-012).
    pub favorite: bool,
    /// Set while archived (PRJ-061).
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required = true)]
    pub archived_at: Option<OffsetDateTime>,
    /// Set while being deleted, for those who may delete it (PRJ-065).
    #[schema(required = true)]
    pub deletion: Option<Deletion>,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Which part of the project list (PRJ-013).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSection {
    /// Active projects, the caller's favorites first.
    #[default]
    Active,
    Archived,
    /// Being deleted; only those the caller may delete.
    Deleting,
}

/// Filters of the project list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProjectQuery {
    /// Part of the name or the start of the key, in any letter case.
    pub q: Option<String>,
    #[param(inline)]
    pub section: Option<ProjectSection>,
    /// Only projects whose team includes the caller, directly or through a group.
    pub in_teams: Option<bool>,
}

/// A project to create (PRJ-017).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewProject {
    pub template: TemplateKey,
    /// Up to 100 characters, unique ignoring case.
    pub name: String,
    /// Latin letters, digits and `_`, starting with a letter, 2–10 characters; stored in capitals.
    /// Absent or empty: suggested from the name.
    pub key: Option<String>,
    /// Number of the first issue; 1 when absent.
    pub first_number: Option<i64>,
    #[serde(default)]
    pub description: String,
}

/// What to check before creating (PRJ-018).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProjectCheckQuery {
    pub name: Option<String>,
    pub key: Option<String>,
}

/// The result of a check: a free key for the name and the problems of what was typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectCheck {
    /// For a valid name.
    #[schema(required = true)]
    pub suggested_key: Option<String>,
    /// Problems with `name` and `key`, as in an `invalid_request` answer.
    pub fields: Vec<FieldError>,
}

/// Changes of the general settings (PRJ-038, PRJ-039); absent fields stay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectPatch {
    pub name: Option<String>,
    /// A new key; the old one keeps leading to the project.
    pub key: Option<String>,
    pub description: Option<String>,
    /// `null` clears the default audience.
    #[serde(
        default,
        deserialize_with = "crate::patch::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<i64>, nullable)]
    pub default_audience_id: Option<Option<i64>>,
    /// The version the change was made on.
    pub version: i64,
}

/// A new owner (PRJ-053).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OwnerChange {
    pub user_id: i64,
}

/// The people of a project (PRJ-046).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectPeople {
    #[schema(required = true)]
    pub owner: Option<UserRef>,
    pub team: GroupRef,
    /// Accounts in the team, with the members of its groups.
    pub team_size: i64,
    /// Groups in the team; all their members are in it.
    pub team_groups: Vec<GroupRef>,
    /// Roles held in the project: by the team (team roles) and directly.
    pub assignments: Vec<Assignment>,
    /// Global roles that reach the project; they change on their holder's page.
    pub global: Vec<Assignment>,
}

/// Accounts and groups to add to a team or remove from it (PRJ-048, PRJ-050).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TeamChange {
    #[serde(default)]
    pub users: Vec<i64>,
    #[serde(default)]
    pub groups: Vec<i64>,
}
