//! Permissions, roles, groups, role assignments, capabilities and the caller's effective access
//! (ACC-021…ACC-066).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::accounts::UserRef;

/// A permission of the fixed catalog. Keys are stable public identifiers.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
pub enum PermissionKey {
    #[serde(rename = "system.read")]
    SystemRead,
    #[serde(rename = "system.write")]
    SystemWrite,
    #[serde(rename = "user.directory")]
    UserDirectory,
    #[serde(rename = "user.details")]
    UserDetails,
    #[serde(rename = "user.self")]
    UserSelf,
    #[serde(rename = "user.update")]
    UserUpdate,
    #[serde(rename = "user.create")]
    UserCreate,
    #[serde(rename = "user.delete")]
    UserDelete,
    #[serde(rename = "project.create")]
    ProjectCreate,
    #[serde(rename = "project.basic")]
    ProjectBasic,
    #[serde(rename = "project.full")]
    ProjectFull,
    #[serde(rename = "project.update")]
    ProjectUpdate,
    #[serde(rename = "project.delete")]
    ProjectDelete,
    #[serde(rename = "issue.read")]
    IssueRead,
    #[serde(rename = "issue.create")]
    IssueCreate,
    #[serde(rename = "issue.update")]
    IssueUpdate,
    #[serde(rename = "issue.delete")]
    IssueDelete,
    #[serde(rename = "issue.link")]
    IssueLink,
    #[serde(rename = "issue.private.read")]
    IssuePrivateRead,
    #[serde(rename = "issue.private.update")]
    IssuePrivateUpdate,
    #[serde(rename = "issue.watchers.read")]
    IssueWatchersRead,
    #[serde(rename = "issue.watchers.update")]
    IssueWatchersUpdate,
    #[serde(rename = "issue.voters.read")]
    IssueVotersRead,
    #[serde(rename = "issue.silent")]
    IssueSilent,
    #[serde(rename = "visibility.bypass")]
    VisibilityBypass,
    #[serde(rename = "comment.create")]
    CommentCreate,
    #[serde(rename = "comment.read")]
    CommentRead,
    #[serde(rename = "comment.update.own")]
    CommentUpdateOwn,
    #[serde(rename = "comment.delete.own")]
    CommentDeleteOwn,
    #[serde(rename = "comment.update.any")]
    CommentUpdateAny,
    #[serde(rename = "comment.delete.any")]
    CommentDeleteAny,
    #[serde(rename = "attachment.create")]
    AttachmentCreate,
    #[serde(rename = "attachment.update")]
    AttachmentUpdate,
    #[serde(rename = "attachment.delete")]
    AttachmentDelete,
}

impl PermissionKey {
    /// The key as text, such as `issue.read`.
    pub fn key(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    pub fn parse(key: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(key.to_owned())).ok()
    }
}

/// What a permission is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionEntity {
    System,
    User,
    Project,
    Issue,
    Comment,
    Attachment,
    Visibility,
}

/// What a permission lets one do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOperation {
    Read,
    Create,
    Update,
    Delete,
    Link,
}

/// One permission of the catalog (ACC-034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PermissionInfo {
    pub key: PermissionKey,
    pub entity: PermissionEntity,
    pub operation: PermissionOperation,
    /// Works only when the role is assigned globally (ACC-044).
    pub global_only: bool,
    /// Switched on together with this one (ACC-046).
    pub implies: Vec<PermissionKey>,
    /// Switched off together with this one (ACC-046).
    pub dependents: Vec<PermissionKey>,
}

/// The whole permission catalog, in catalog order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PermissionCatalog {
    pub permissions: Vec<PermissionInfo>,
}

/// A role that comes with every installation; shown in the viewer's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PredefinedRoleKey {
    SystemAdmin,
    ProjectAdmin,
    ProjectMember,
    DirectoryViewer,
    ProjectCreator,
    AccountCreator,
}

/// A role as a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RoleRef {
    pub id: i64,
    /// English name; predefined roles are shown by `builtin` in the viewer's language.
    pub name: String,
    #[schema(required = true)]
    pub builtin: Option<PredefinedRoleKey>,
}

/// A role with its permissions (ACC-053).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Role {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// Set for predefined roles, which cannot change.
    #[schema(required = true)]
    pub builtin: Option<PredefinedRoleKey>,
    /// Closed under implication, in catalog order.
    pub permissions: Vec<PermissionKey>,
    /// How many accounts and groups hold it, at any scope.
    pub assignments: i64,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Filters of the roles list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RoleQuery {
    /// Part of the name, in any letter case.
    pub q: Option<String>,
    /// Only roles with this permission.
    pub permission: Option<PermissionKey>,
}

/// A custom role to create (ACC-054).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewRole {
    /// Up to 100 characters; not a predefined role's name.
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// What it implies is added (ACC-046).
    #[serde(default)]
    pub permissions: Vec<PermissionKey>,
}

/// Changes of a custom role's name or description; absent fields stay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RolePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    /// The version the change was made on.
    pub version: i64,
}

/// A built-in group, shown in the viewer's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinGroupKey {
    /// Every account, the guest included.
    Everyone,
    /// Every account except the guest.
    AllMembers,
}

/// A group as a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupRef {
    pub id: i64,
    /// A team's name is its project's name.
    pub name: String,
    #[schema(required = true)]
    pub builtin: Option<BuiltinGroupKey>,
    /// The project whose team this is (PRJ-047); its members change on that project's people page.
    #[schema(required = true)]
    pub team_of: Option<i64>,
}

/// Who holds an assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Holder {
    User { user: UserRef },
    Group { group: GroupRef },
}

/// A role held by an account or a group at one scope (ACC-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Assignment {
    pub id: i64,
    pub role: RoleRef,
    pub holder: Holder,
    /// `null` for global scope.
    #[schema(required = true)]
    pub project_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Whose assignments to list: exactly one of the two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AssignmentQuery {
    pub user: Option<i64>,
    pub group: Option<i64>,
}

/// Assigns a role to an account or a group at one or more scopes (ACC-059, ACC-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewAssignment {
    pub role_id: i64,
    /// Exactly one of `user_id` and `group_id`.
    pub user_id: Option<i64>,
    pub group_id: Option<i64>,
    /// Assign globally.
    #[serde(default)]
    pub global: bool,
    /// Assign in these projects.
    #[serde(default)]
    pub projects: Vec<i64>,
}

/// Assignments that were made; existing ones are not repeated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Assignments {
    pub items: Vec<Assignment>,
}

/// A group of the list or the group page (ACC-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub description: String,
    #[schema(required = true)]
    pub builtin: Option<BuiltinGroupKey>,
    /// Every new account joins it.
    pub auto_join: bool,
    /// The project whose team this is (PRJ-047).
    #[schema(required = true)]
    pub team_of: Option<i64>,
    /// Accounts in it; for a team, with the members of its groups.
    pub members: i64,
    /// The group's own role assignments.
    pub roles: Vec<GroupRole>,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// A role a group holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupRole {
    pub assignment_id: i64,
    pub role: RoleRef,
    #[schema(required = true)]
    pub project_id: Option<i64>,
}

/// Search in a list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TextQuery {
    /// Part of the name, in any letter case.
    pub q: Option<String>,
}

/// Which groups the list shows (ACC-022).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    /// Groups of people, the built-in ones included.
    #[default]
    Groups,
    /// Project teams.
    Teams,
    /// Both.
    All,
}

/// Filters of the groups list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GroupQuery {
    /// Part of the name, in any letter case.
    pub q: Option<String>,
    /// Groups of people (default), project teams, or both.
    #[param(inline)]
    pub kind: Option<GroupKind>,
}

/// A group to create (ACC-023).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewGroup {
    /// Up to 100 characters, unique.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub auto_join: bool,
}

/// Changes of a group's settings (ACC-024); absent fields stay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub auto_join: Option<bool>,
    pub version: i64,
}

/// Accounts to add to a group or remove from it (ACC-026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MembersChange {
    /// Up to 100 account IDs.
    pub users: Vec<i64>,
}

/// The accounts a change applied to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ChangedMembers {
    /// Accounts that were added or removed; the others were already so.
    pub users: Vec<i64>,
}

/// Groups to delete and the one that takes their place (ACC-032).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupDeletion {
    pub groups: Vec<i64>,
    pub replacement: i64,
}

/// Groups whose deletion to preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupSelection {
    pub groups: Vec<i64>,
}

/// What deleting groups takes away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupDeletionImpact {
    pub groups: i64,
    /// Memberships that end.
    pub memberships: i64,
    /// Role assignments that are revoked.
    pub assignments: i64,
}

/// A feature switched on per group (ACC-064).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKey {
    /// Changing who may view, use and edit shared views (ACC-066).
    ShareViews,
}

/// A capability and the groups it is on for; no groups means off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Capability {
    pub key: CapabilityKey,
    pub groups: Vec<GroupRef>,
    /// Whether the group list may be empty.
    pub can_be_switched_off: bool,
}

/// Every capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Capabilities {
    pub items: Vec<Capability>,
}

/// The groups a capability is on for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CapabilityChange {
    pub groups: Vec<i64>,
}

/// Permissions held in one project beyond the global ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectAccess {
    pub project_id: i64,
    pub permissions: Vec<PermissionKey>,
}

/// The caller's effective access (ACC-062): the UI hides what is not available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AccessMap {
    /// Held everywhere: global permissions, and project permissions in every project.
    pub global: Vec<PermissionKey>,
    pub projects: Vec<ProjectAccess>,
    pub capabilities: Vec<CapabilityKey>,
}

/// Where a role a person holds comes from (ACC-061).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RoleSource {
    /// Assigned to the person; this assignment can be revoked.
    Direct { assignment_id: i64 },
    /// Through a group, a built-in one included.
    Group { group: GroupRef },
}

/// One role a person holds at one scope, with all its sources (ACC-018, ACC-061).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EffectiveRole {
    pub role: RoleRef,
    #[schema(required = true)]
    pub project_id: Option<i64>,
    /// The permissions it gives at this scope.
    pub permissions: Vec<PermissionKey>,
    pub sources: Vec<RoleSource>,
}

/// Every role a person holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EffectiveRoles {
    pub items: Vec<EffectiveRole>,
}
