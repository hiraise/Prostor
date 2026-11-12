//! Sign-in, sessions, the caller's account and personal API tokens (AUTH-001…AUTH-005, AUTH-009,
//! AUTH-029, AUTH-030).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

/// Password sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SignInRequest {
    /// Username or email, in any letter case.
    pub login: String,
    pub password: String,
    /// Keep the session for the remember-me duration instead of ending it after the idle
    /// timeout. Off on shared computers.
    #[serde(default)]
    pub remember: bool,
}

/// The caller's browser session. The session itself travels in an HTTP-only cookie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Session {
    pub user: CurrentUser,
    /// Signed in with "remember me".
    pub remember: bool,
    /// When the session started.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// A new password must be chosen before anything else works (AUTH-010).
    pub password_change_required: bool,
}

/// The caller's own account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub full_name: String,
    #[schema(required = true)]
    pub email: Option<String>,
    pub email_verified: bool,
    /// Background of the generated avatar: 0–7.
    pub avatar_color: i16,
    /// The uploaded avatar (ACC-017); `null` shows the generated one.
    #[schema(required = true)]
    pub avatar_url: Option<String>,
    pub version: i64,
}

/// A new password for the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    /// From 8 to 256 characters.
    pub new_password: String,
}

/// What a personal API token may be used for. A token never has more rights than its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum TokenScope {
    /// Read projects, issues, boards, reports and everything else of the tracker.
    #[serde(rename = "tracker.read")]
    TrackerRead,
    /// Create and change them; includes `tracker.read`.
    #[serde(rename = "tracker.write")]
    TrackerWrite,
    /// Read users, groups, roles and settings.
    #[serde(rename = "admin.read")]
    AdminRead,
    /// Change them; includes `admin.read`.
    #[serde(rename = "admin.write")]
    AdminWrite,
}

/// A personal API token. Its secret is shown only once, when it is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiToken {
    pub id: i64,
    pub name: String,
    /// The fewest scopes that say what the token may do: a write scope includes its read scope.
    pub scopes: Vec<TokenScope>,
    /// Who created it: the owner, or an administrator for a bot account.
    #[schema(required = true)]
    pub created_by: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Last request made with it, accurate to a minute; `null` if never used.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required = true)]
    pub last_used_at: Option<OffsetDateTime>,
    pub version: i64,
}

/// A token to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewApiToken {
    /// What the token is for, such as the name of a script; up to 100 characters.
    pub name: String,
    /// At least one.
    pub scopes: Vec<TokenScope>,
}

/// A created token with its secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreatedApiToken {
    pub token: ApiToken,
    /// Send as `Authorization: Bearer <secret>`. Shown only now; store it safely.
    pub secret: String,
}
