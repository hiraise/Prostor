//! Accounts as people see each other and as administrators manage them (ACC-002…ACC-017), and the
//! emailed one-time links: password reset, email verification, invitations (AUTH-008, AUTH-014,
//! ACC-004).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::access::GroupRef;
use crate::projects::ProjectRef;

/// How a person appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UserRefKind {
    /// With their real name.
    Person,
    /// The viewer may not see the directory: a pseudonym stands in (ACC-016).
    Masked,
    /// The account was deleted (ACC-006).
    Deleted,
}

/// A person wherever they are mentioned: authors, members, holders of roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UserRef {
    pub id: i64,
    /// The real values for `person`; English stand-ins otherwise.
    pub username: String,
    pub full_name: String,
    /// Background of the generated avatar: 0–7.
    pub avatar_color: i16,
    /// The uploaded avatar (ACC-017); `null` shows the generated one. Only for `person`.
    #[schema(required = true)]
    pub avatar_url: Option<String>,
    pub kind: UserRefKind,
    /// The pseudonym number of a `masked` person, stable for this viewer.
    #[schema(required = true)]
    pub pseudonym: Option<u32>,
    /// The account is disabled.
    pub disabled: bool,
}

/// How much of a profile the caller sees (ACC-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileTier {
    /// ID, username, name and avatar.
    Basic,
    /// Also email, groups and roles.
    Details,
    /// Everything, the password state included: the owner, or an administrator of accounts.
    Full,
}

/// The word shown with a disabled account; every label has the same effect (ACC-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DisabledLabel {
    Disabled,
    Suspended,
    /// Left the organization.
    Left,
    Blocked,
}

/// Why and since when an account is disabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DisabledState {
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    #[schema(required = true)]
    pub reason: Option<String>,
    pub label: DisabledLabel,
}

/// An account in the directory and in people pickers (ACC-002, ACC-070).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AccountSummary {
    pub id: i64,
    pub username: String,
    pub full_name: String,
    pub avatar_color: i16,
    /// The uploaded avatar (ACC-017); `null` shows the generated one.
    #[schema(required = true)]
    pub avatar_url: Option<String>,
    /// `null` when the caller may not see it (ACC-015) or there is none.
    #[schema(required = true)]
    pub email: Option<String>,
    pub email_verified: bool,
    /// The guest account, which stands for anonymous visitors.
    pub guest: bool,
    /// Created by an invitation that has not been accepted yet (ACC-004).
    pub invited: bool,
    #[schema(required = true)]
    pub disabled: Option<DisabledState>,
    /// The groups the account was added to (built-in groups are implied). Empty when the caller
    /// may not see them.
    pub groups: Vec<GroupRef>,
    /// Projects whose team includes the account, directly or through a group, among those the
    /// caller may see (ACC-002). Empty when the caller may not see groups.
    pub projects: Vec<ProjectRef>,
    /// The caller may open the full profile, with personal settings and security (ACC-015).
    pub full_profile: bool,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// A profile (ACC-009, ACC-015, ACC-017): the summary, the tier, and with the full tier the
/// password state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Account {
    #[serde(flatten)]
    pub summary: AccountSummary,
    pub tier: ProfileTier,
    /// Only with the full tier.
    #[schema(required = true)]
    pub password: Option<PasswordState>,
}

/// Whether the account has a password and must change it (AUTH-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PasswordState {
    pub has_password: bool,
    pub must_change: bool,
}

/// Which accounts the directory lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    #[default]
    All,
    Enabled,
    Disabled,
}

/// Filters of the directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DirectoryQuery {
    /// Start of the full name (or of a word in it), username, email or email domain.
    pub q: Option<String>,
    /// Default `all`.
    #[param(inline)]
    pub state: Option<AccountState>,
}

/// An account an administrator creates (ACC-003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewAccount {
    pub full_name: String,
    pub username: String,
    pub email: String,
    /// From 8 to 256 characters.
    pub password: String,
    /// Ask for a new password at the first sign-in (AUTH-010).
    #[serde(default)]
    pub must_change_password: bool,
}

/// People to invite by email (ACC-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InvitationRequest {
    /// Addresses separated by spaces, commas or new lines; up to 100.
    pub emails: String,
}

/// The accounts created for invited people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Invited {
    pub items: Vec<AccountSummary>,
}

/// Changes of identity fields (ACC-009, ACC-017); absent fields stay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AccountPatch {
    pub username: Option<String>,
    pub full_name: Option<String>,
    /// A new address starts unverified and gets a verification link.
    pub email: Option<String>,
    /// Needed when people change their own email and have a password.
    pub current_password: Option<String>,
    pub version: i64,
}

/// Accounts to disable (ACC-005).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DisableRequest {
    /// Up to 100 account IDs.
    pub users: Vec<i64>,
    #[serde(default)]
    pub reason: String,
    pub label: DisabledLabel,
}

/// Accounts an action applies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UserSelection {
    /// Up to 100 account IDs.
    pub users: Vec<i64>,
}

/// Accounts to delete and the account that takes their place (ACC-006).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UserDeletion {
    pub users: Vec<i64>,
    pub replacement: i64,
}

/// What deleting accounts takes away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UserDeletionImpact {
    pub accounts: i64,
    pub sessions: i64,
    pub tokens: i64,
    pub memberships: i64,
    pub assignments: i64,
}

/// Where an email went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EmailSent {
    pub email: String,
}

/// Asks for a password-reset link (AUTH-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PasswordResetRequest {
    pub email: String,
}

/// The secret of an emailed link: the part of its address after `#`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkToken {
    pub token: String,
}

/// Why a link was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkPurpose {
    PasswordReset,
    EmailVerification,
    Invitation,
}

/// What a link is for, before it is used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkInfo {
    pub purpose: LinkPurpose,
    pub email: String,
    pub username: String,
    pub full_name: String,
    /// The link no longer works; a new one is needed.
    pub expired: bool,
}

/// A new password from a reset link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PasswordResetConfirm {
    pub token: String,
    /// From 8 to 256 characters.
    pub new_password: String,
}

/// Joining from an invitation (ACC-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InvitationAcceptance {
    pub token: String,
    /// From 8 to 256 characters.
    pub password: String,
    /// Replaces the name the invitation started with.
    pub full_name: Option<String>,
    /// Replaces the username the invitation started with.
    pub username: Option<String>,
}
