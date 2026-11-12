//! Notifications (NTF, plan step 1.10): personal settings, subscriptions on tags and saved
//! searches, the watcher list of an issue, and the inbox with its folders, threads and read
//! state (1.10a-3).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::accounts::UserRef;

/// The kinds of events (NTF-022). `system` is what the server itself tells a person (job
/// reports); it never appears in a subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    Created,
    Updated,
    Resolved,
    Comment,
    Vote,
    TagAdded,
    TagRemoved,
    System,
}

/// What a subscription listens to (NTF-021): a tag or a saved search, by ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionSourceKind {
    Tag,
    Search,
}

/// A person's notification settings (NTF-017, NTF-030, NTF-034, NTF-066): the email channel and
/// its form, own changes, mentions, the kinds of the built-in watch subscription, and the
/// auto-watch switches (ISS-030). The inbox channel is always on, so it has no switch here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NotificationSettings {
    pub email: bool,
    pub email_html: bool,
    pub own_changes: bool,
    pub mentions: bool,
    pub watch_created: bool,
    pub watch_updated: bool,
    pub watch_resolved: bool,
    pub watch_comment: bool,
    pub watch_vote: bool,
    pub watch_tag: bool,
    /// Auto-watch (ISS-030): watch the issues I create, comment on, am assigned to, vote for.
    pub autowatch_created: bool,
    pub autowatch_commented: bool,
    pub autowatch_assigned: bool,
    pub autowatch_voted: bool,
}

/// One subscription (NTF-021): its source with the name for showing, and the kinds it delivers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NotificationSubscription {
    pub id: i64,
    pub source: SubscriptionSourceKind,
    /// The tag's or the saved search's ID.
    pub source_id: i64,
    /// The tag's or the saved search's current name.
    pub name: String,
    pub kinds: Vec<NotificationKind>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// The caller's subscriptions, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct NotificationSubscriptions {
    pub subscriptions: Vec<NotificationSubscription>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NewNotificationSubscription {
    pub source: SubscriptionSourceKind,
    pub source_id: i64,
    pub kinds: Vec<NotificationKind>,
}

/// Changes the kinds of a subscription (NTF-024): an empty list keeps it stored but delivering
/// nothing; removing it is the delete request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NotificationSubscriptionPatch {
    pub kinds: Vec<NotificationKind>,
}

/// Who watches an issue (NTF-015), behind `issue.watchers.read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct IssueWatchers {
    pub users: Vec<UserRef>,
}

/// The whole new watcher list of an issue (NTF-016): the caller included; anyone else needs
/// `issue.watchers.update`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct WatchersChange {
    pub users: Vec<i64>,
}

/// The parameters of the inbox (NTF-048, NTF-049): its folder, only unread rows, the keyset
/// cursor of the previous page, one issue's whole thread, and the page size.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InboxQuery {
    /// `all` (default), `direct` (mentions), `subscriptions` (tags and saved searches) or
    /// `system` (only people who see the administration area).
    pub folder: Option<String>,
    /// `only`: leave read rows out.
    pub unread: Option<String>,
    /// The previous page's `next` cursor: rows before it.
    pub before: Option<i64>,
    /// One issue's whole thread, regardless of paging.
    pub issue: Option<i64>,
    /// Threads per page, 1…100; default 25.
    pub limit: Option<i64>,
}

/// The issue a row names: its readable ID (`WEB-12`) and summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct NotificationIssue {
    pub readable_id: String,
    pub summary: String,
}

/// One rendered line of a row: plain text, a field change or a comment excerpt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum NotificationLine {
    Text {
        text: String,
    },
    Field {
        name: String,
        old: String,
        new: String,
    },
    Comment {
        id: i64,
        excerpt: String,
    },
}

/// What an issue row says: the issue and the lines its reader may see, rendered at fire time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct NotificationChange {
    pub issue: NotificationIssue,
    pub lines: Vec<NotificationLine>,
    pub excerpt: String,
}

/// What a `system` row says: a job report. `state` is a stable key (`finished`, `failed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct NotificationReport {
    pub job: String,
    pub state: String,
    pub message: Option<String>,
}

/// One row of the inbox (NTF-049): who caused it, why it reached the reader, and what it says.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct InboxNotification {
    pub id: i64,
    pub kind: NotificationKind,
    /// The issue the row is about; `null` for `system` rows.
    pub issue_id: Option<i64>,
    /// What the row says: the change of an issue, or a `system` report.
    pub content: NotificationBody,
    /// Why the row reached the reader: `watch`, `mention`, `tag:<id>`, `search:<id>`.
    pub reasons: Vec<String>,
    /// The last row before the reader's access to the issue was lost («наблюдение
    /// прекращено», NTF-043); no later rows of that issue arrive until access returns.
    pub last: bool,
    pub read: bool,
    pub author: UserRef,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// What a row says, in one of the two shapes rows have.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(untagged)]
pub enum NotificationBody {
    Change(NotificationChange),
    Report(NotificationReport),
}

/// One thread of the inbox (NTF-049): the head — the thread's newest row — with the thread's
/// newest rows inline, the head first. `system` rows stand alone.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct InboxThread {
    pub head: InboxNotification,
    pub latest: Vec<InboxNotification>,
}

/// One page of the inbox, threads newest first.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Inbox {
    pub threads: Vec<InboxThread>,
    /// `before` of the next page; `null` on the last.
    pub next: Option<i64>,
}

/// The unread counts of the folders (the badge, NTF-048).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct InboxSummary {
    pub total: i64,
    pub direct: i64,
    pub subscriptions: i64,
    pub system: i64,
}

/// Marks rows read or unread (NTF-050): exactly one selector — these `ids`, one `issue` thread
/// or `all`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NotificationRead {
    pub ids: Option<Vec<i64>>,
    pub issue: Option<i64>,
    pub all: Option<bool>,
    /// `true` marks the rows unread again; the default marks them read.
    #[serde(default)]
    pub unread: bool,
}
