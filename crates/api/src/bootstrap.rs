//! The configuration every client loads before rendering (ADM-002).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::preferences::HomePage;

/// What the web app needs before it renders anything. Available without signing in,
/// so it never contains anything sensitive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Bootstrap {
    /// Server version.
    pub version: String,
    /// Canonical address of the instance, if the administrator has set it.
    #[schema(required = true)]
    pub base_url: Option<String>,
    /// Nobody has run the first-run setup yet: the app opens the setup wizard.
    pub setup_required: bool,
    /// Outgoing email works: invitations, password resets and verifications can be sent.
    pub email_enabled: bool,
    /// Avatars can be taken from Gravatar (ACC-017).
    pub gravatar: bool,
    /// The page a person without a preference lands on after sign-in (UI-004).
    pub default_home: HomePage,
    /// How dates and times are shown to people without a preference of their own (ADM-016).
    pub default_date_format: DateFormat,
    pub limits: Limits,
    /// How durations are read and shown (TIME-005, TIME-006).
    pub work_schedule: WorkSchedule,
}

/// One of the four ways dates and times are shown (ADM-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    /// By the interface language, today's behavior.
    Locale,
    /// `31.12.2000, 23:59`.
    DmyDot,
    /// `31 дек. 2000, 23:59` — the month per the viewer's language.
    DmyName,
    /// `2000-12-31 23:59`.
    Iso,
}

/// The work day and week durations are counted in: `1d` is a work day, `1w` a work week.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WorkSchedule {
    /// Minutes in a work day.
    pub day_minutes: i64,
    /// Work days in a week.
    pub week_days: i64,
}

/// Limits the browser checks before sending a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Limits {
    /// Largest file one upload may have, in bytes: the administrators' setting within the
    /// server's ceiling (CNT-019).
    pub upload_max_bytes: u64,
}
