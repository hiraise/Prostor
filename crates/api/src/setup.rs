//! The first-run setup wizard (ADM-047).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Everything the setup wizard asks for. Problems are reported per field (`invalid_request`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SetupRequest {
    /// The setup code from the server log; case, spaces and hyphens do not matter.
    pub code: String,
    /// The administrator's sign-in name: Latin letters, digits, `.`, `_`, `-`.
    pub username: String,
    pub full_name: String,
    pub email: String,
    /// From 8 to 256 characters.
    pub password: String,
    /// Language of query keywords, built-in names and notifications.
    pub system_language: Language,
    /// The address people open trackflow at, used in links in emails.
    #[schema(required = true)]
    pub base_url: Option<String>,
    /// Default time zone, an IANA name such as `Europe/Berlin`.
    pub time_zone: String,
}

/// Languages trackflow speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    En,
    Ru,
}
