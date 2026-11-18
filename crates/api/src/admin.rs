//! Instance administration (ADM): the admin mail settings (NTF-002, NTF-004, NTF-006, NTF-007;
//! plan step 1.10 §5.1) — what the screen of «Администрирование» → «Почта» takes and answers;
//! the general settings (plan step 1.11 §3.1–§3.2) and the jobs of «Задания» (§3.3).
//! Secret values never travel back: only whether one is stored.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::accounts::UserRef;

/// Where the outgoing mail transport of the instance comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailSource {
    /// The admin mail settings (NTF-004).
    Settings,
    /// The environment (`TASKMASTER_SMTP_URL`, D-026): no mail setting is stored.
    Env,
    /// No transport at all.
    None,
}

/// The mail settings as the administration screen sees them (NTF-004). No secret value is ever
/// part of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MailSettings {
    /// Where the transport of this node comes from.
    pub source: MailSource,
    /// Whether a transport exists at all, switch or not.
    pub configured: bool,
    /// The instance email switch as it stands (NTF-002): while off, nothing is sent or queued.
    pub email_enabled: bool,
    /// The sender of every email (NTF-006); empty when nothing is set.
    pub mail_from: String,
    /// Where replies go; empty means replies go to the sender (NTF-006).
    pub mail_reply_to: String,
    /// Whether an SMTP URL is stored; its value is a secret.
    pub has_smtp_url: bool,
    /// Whether an SMTP user name is stored; its value is a secret.
    pub has_username: bool,
    /// Whether an SMTP password is stored; its value is a secret.
    pub has_password: bool,
}

/// A change of the mail settings (NTF-004). Secret fields are write-only: the field absent keeps
/// the stored value, `null` clears it, a string stores it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MailSettingsChange {
    /// `smtp://host:port` or `smtps://host:port`, credentials optional in the URL.
    pub smtp_url: Option<Option<String>>,
    /// The SMTP user name, when the server wants one the URL does not carry.
    pub smtp_username: Option<Option<String>>,
    /// The SMTP password (a provider's app password when 2-step verification is on).
    pub smtp_password: Option<Option<String>>,
    /// The sender of every email (NTF-006); empty turns to the environment's, else the default.
    pub mail_from: Option<String>,
    /// Where replies go; empty means replies go to the sender (NTF-006).
    pub mail_reply_to: Option<String>,
    /// The instance email switch (NTF-002); switching on needs a working transport.
    pub email_enabled: Option<bool>,
}

/// Where a test message goes (NTF-007).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MailTestSend {
    /// Any address; the transport's complaint, if any, comes back in full.
    pub to: String,
}

// ---- The general settings (plan step 1.11 §3.1–§3.2) ----

/// The non-secret settings of the instance as the «Настройки» screen sees them: value, default
/// and whether a row is stored at all. The mail settings are not here — they have their own
/// screen and API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AdminSettings {
    /// Every non-secret, non-mail setting, in the registry's order.
    pub settings: Vec<AdminSetting>,
    /// The instance-wide flags (ADM-023); empty while none exist.
    pub flags: Vec<AdminFlag>,
    /// The deployment's `TASKMASTER_UPLOAD_CEILING_BYTES`: the upload setting may not exceed it.
    pub upload_ceiling: u64,
}

/// One setting of the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AdminSetting {
    /// The registry key, the same the change set takes.
    pub key: String,
    /// The value now in force: the stored row, else the default.
    pub value: Value,
    /// The default the setting falls back to.
    pub default: Value,
    /// Whether a row is stored (the value differs from the default).
    pub is_stored: bool,
}

/// One instance-wide flag of the capabilities screen (ADM-023).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AdminFlag {
    /// The bare name, without the `flag.` prefix of the stored key.
    pub key: String,
    /// What the flag switches, one line for the screen.
    pub description: String,
    /// The value now in force: the stored row, else the default.
    pub value: bool,
    /// The value when no row is stored.
    pub default: bool,
    /// Whether a row is stored.
    pub is_stored: bool,
    /// Whether an administrator may switch the flag off.
    pub may_switch_off: bool,
}

/// A change set of the general settings: each present key changes, `null` returns it to the
/// default, absent keys stay. Only keys of [`AdminSettings::settings`] belong here; the mail
/// settings have their own API.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(value_type = Object)]
pub struct AdminSettingsChange(pub BTreeMap<String, Option<Value>>);

// ---- The jobs of «Задания» (plan step 1.11 §3.3) ----

/// Query parameters of the admin jobs list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AdminJobsQuery {
    /// Only jobs in this state.
    pub state: Option<AdminJobState>,
    /// Words matched against the kind and the initiator's name; a leading `-` excludes.
    pub q: Option<String>,
    /// Only jobs with a smaller id: keyset pagination, newest first.
    pub before: Option<i64>,
    /// Items per page, from 1 to 200; default 50.
    pub limit: Option<u32>,
}

/// A page of jobs, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AdminJobs {
    pub items: Vec<AdminJob>,
    /// The id to pass as `before` for the next page; `None` when this page is the last.
    #[schema(required = true)]
    pub next: Option<i64>,
}

/// A job as the admin screen sees it (ADM-031).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AdminJob {
    pub id: i64,
    /// The machine kind, such as `projects.delete`; the screen translates it.
    pub kind: String,
    pub state: AdminJobState,
    /// Who started the job; `None` when the server itself did.
    #[schema(required = true)]
    pub initiator: Option<UserRef>,
    /// Whether this kind of job may be cancelled while it runs.
    pub cancellable: bool,
    /// Whether someone asked the running job to stop.
    pub cancel_requested: bool,
    /// Runs so far, counting from 0; a run interrupted by shutdown does not count.
    pub attempts: i32,
    /// Runs before the job is marked failed.
    pub max_attempts: i32,
    pub progress: JobProgress,
    /// The latest status line of the handler.
    #[schema(required = true)]
    pub status_text: Option<String>,
    /// Why the job failed; `None` unless it did.
    #[schema(required = true)]
    pub last_error: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required = true)]
    pub started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required = true)]
    pub finished_at: Option<OffsetDateTime>,
}

/// How far a job has come (ADM-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JobProgress {
    /// Steps done so far.
    pub done: i64,
    /// Steps in total; `None` when the job cannot tell yet.
    #[schema(required = true)]
    pub total: Option<i64>,
}

/// The state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminJobState {
    Queued,
    Running,
    Finished,
    Failed,
    Cancelled,
}
