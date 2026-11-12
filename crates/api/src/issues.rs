//! Issues (ISS), their activity stream (HIST) and tags: what the issue endpoints take and answer.

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};

use crate::access::GroupRef;
use crate::accounts::UserRef;
use crate::comments::Comment;
use crate::content::TextRefs;
use crate::fields::FieldRef;
use crate::links::{IssueBrief, LoggedLink, NewIssueLink, SubtaskCount};
use crate::projects::ProjectRef;
use crate::sharing::{ListingScope, SharedWith, SharedWithInput, SharingAbilities};

/// A tag as a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TagRef {
    pub id: i64,
    pub name: String,
    /// Color scheme 0–11 of the value palette.
    #[schema(required = true)]
    pub color: Option<i16>,
}

/// A tag the caller owns or that is shared with them (ISS-034, ISS-037).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    #[schema(required = true)]
    pub color: Option<i16>,
    pub owner: UserRef,
    /// Only for the owner, editors and user administrators.
    #[schema(required = true)]
    pub sharing: Option<TagSharing>,
    pub abilities: SharingAbilities,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Who sees a tag on issues, who puts it on and takes it off, who renames and recolors it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TagSharing {
    pub view: SharedWith,
    #[serde(rename = "use")]
    pub use_: SharedWith,
    pub edit: SharedWith,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TagSharingInput {
    #[serde(default)]
    pub view: SharedWithInput,
    #[serde(default, rename = "use")]
    pub use_: SharedWithInput,
    #[serde(default)]
    pub edit: SharedWithInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NewTag {
    pub name: String,
    #[serde(default)]
    pub color: Option<i16>,
}

/// Changes of a tag; absent fields stay, `color: null` clears the color.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct TagPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<i16>)]
    pub color: Option<Option<i16>>,
    /// New audiences: the owner or a user administrator, with share-views (ACC-066).
    #[serde(default)]
    pub sharing: Option<TagSharingInput>,
    /// A new owner: user administrators with share-views.
    #[serde(default)]
    pub owner: Option<i64>,
    /// The version the change is based on.
    pub version: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TagQuery {
    /// Part of the name.
    pub q: Option<String>,
    /// `mine` by default.
    #[param(inline)]
    pub scope: Option<ListingScope>,
}

/// A value of a set as an issue holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HeldValue {
    pub id: i64,
    pub name: String,
    #[schema(required = true)]
    pub color: Option<i16>,
    /// Shown as a hint when the value is picked (FLD-043).
    pub description: String,
    pub archived: bool,
    /// Counts as resolved (state values).
    pub resolved: bool,
}

/// One field of an issue with its value (FLD-001…FLD-014). Only the member of the field's kind
/// is filled; an empty field has none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IssueField {
    pub field: FieldRef,
    /// Shown for an empty value (FLD-056).
    pub empty_label: String,
    /// Needs the private-field permissions (FLD-060).
    pub private: bool,
    /// Must not be empty (FLD-057).
    pub required: bool,
    /// The caller may change it.
    pub editable: bool,
    /// String and text (Markdown) fields.
    #[schema(required = true)]
    pub text: Option<String>,
    /// Text fields rendered (D-031); markers are explained by the issue's `refs`.
    #[schema(required = true)]
    pub html: Option<String>,
    /// Integer fields.
    #[schema(required = true)]
    pub integer: Option<i64>,
    /// Float fields.
    #[schema(required = true)]
    pub float: Option<f64>,
    #[schema(required = true)]
    pub date: Option<Date>,
    #[schema(required = true)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub date_time: Option<OffsetDateTime>,
    /// Duration fields, in minutes.
    #[schema(required = true)]
    pub minutes: Option<i64>,
    /// Enumeration, state, version, build and owned fields, in the set's order.
    pub values: Vec<HeldValue>,
    pub users: Vec<UserRef>,
    pub groups: Vec<GroupRef>,
}

/// Whom an issue is limited to (ISS-041); both empty: no restriction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Audience {
    pub users: Vec<UserRef>,
    pub groups: Vec<GroupRef>,
}

/// What the caller may do with an issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueAbilities {
    /// Change summary, description, public fields, tags and audience.
    pub update: bool,
    /// Change private fields.
    pub update_private: bool,
    pub delete: bool,
    /// Add and remove other watchers.
    pub manage_watchers: bool,
    pub vote: bool,
    /// Change without notifications.
    pub silent: bool,
}

/// An issue or draft in full (ISS-001).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Issue {
    pub id: i64,
    /// `KEY-12`; `null` for a draft. Differs from the ID asked for when the issue moved or its
    /// project's key changed (ISS-003).
    #[schema(required = true)]
    pub readable_id: Option<String>,
    #[schema(required = true)]
    pub number: Option<i64>,
    pub draft: bool,
    pub project: ProjectRef,
    pub summary: String,
    /// Markdown.
    pub description: String,
    /// The description rendered (D-031).
    pub description_html: String,
    /// What the markers of the description and text fields stand for. Filled in answers about
    /// one issue; empty in lists.
    pub refs: TextRefs,
    pub reporter: UserRef,
    /// Publication time; for a draft, when it was started.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[schema(required = true)]
    pub updater: Option<UserRef>,
    /// Set while the issue is resolved (ISS-023).
    #[schema(required = true)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub resolved_at: Option<OffsetDateTime>,
    pub audience: Audience,
    /// The project's fields the caller may see, in the project's order.
    pub fields: Vec<IssueField>,
    /// The caller's tags on it.
    pub tags: Vec<TagRef>,
    /// The caller watches it (ISS-030).
    pub starred: bool,
    pub voted: bool,
    pub votes: i32,
    /// People watching; `null` without `issue.watchers.read`.
    #[schema(required = true)]
    pub watchers: Option<i64>,
    pub can: IssueAbilities,
    /// Its subtasks the caller may see (LNK-014).
    pub subtasks: SubtaskCount,
    /// Its parent through the subtask link type, when the caller may see it (LNK-013). Filled in
    /// answers about one issue; `null` in lists.
    #[schema(required = true)]
    pub parent: Option<IssueBrief>,
    pub version: i64,
    /// Send it with edits of the summary or description (ISS-051).
    pub text_version: i64,
}

/// A value as sent: the field's kind says how it is read. A number or a list of IDs for fields
/// with values (one ID for single fields); a number for numbers and durations (minutes); text for
/// strings, Markdown, dates (`YYYY-MM-DD`), date-times (RFC 3339) and durations (`1h 30m`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum FieldValueInput {
    Integer(i64),
    Float(f64),
    Text(String),
    Ids(Vec<i64>),
}

/// A field value of a new issue.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct IssueFieldInput {
    pub field: i64,
    /// `null` leaves the field empty instead of its default.
    #[schema(required = true)]
    pub value: Option<FieldValueInput>,
}

/// An audience as sent (ISS-041).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AudienceInput {
    #[serde(default)]
    pub users: Vec<i64>,
    #[serde(default)]
    pub groups: Vec<i64>,
}

/// A new issue (ISS-005…ISS-007).
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct NewIssue {
    pub project: i64,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    /// Values instead of the project's defaults.
    #[serde(default)]
    pub fields: Vec<IssueFieldInput>,
    /// Omit for the project's default audience.
    #[serde(default)]
    pub audience: Option<AudienceInput>,
    /// The caller's tags to put on it.
    #[serde(default)]
    pub tags: Vec<i64>,
    /// People to watch it; others than the caller need `issue.watchers.update`.
    #[serde(default)]
    pub watchers: Vec<i64>,
    /// Keep it as a draft instead of publishing.
    #[serde(default)]
    pub draft: bool,
    /// The caller's staged uploads to attach (ISS-007, CNT-017).
    #[serde(default)]
    pub uploads: Vec<i64>,
    /// Links to existing issues (ISS-009, LNK-006, LNK-007), such as the parent of a new subtask.
    #[serde(default)]
    pub links: Vec<NewIssueLink>,
}

/// Changes of texts and audience (ISS-018, ISS-041); absent fields stay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, ToSchema)]
pub struct IssuePatch {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// The `text_version` the edit started from; required with a summary or description.
    #[serde(default)]
    pub text_version: Option<i64>,
    #[serde(default)]
    pub audience: Option<AudienceInput>,
    /// Without notifications (needs `issue.silent`).
    #[serde(default)]
    pub silent: bool,
}

/// A new value of one field (ISS-019).
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct FieldValueChange {
    /// `null` clears the field.
    #[schema(required = true)]
    pub value: Option<FieldValueInput>,
    #[serde(default)]
    pub silent: bool,
}

/// Values to add to or remove from a multi-valued field (FLD-001).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct FieldValuesChange {
    pub ids: Vec<i64>,
    #[serde(default)]
    pub silent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct MoveIssue {
    pub project: i64,
    #[serde(default)]
    pub silent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MovePreviewQuery {
    /// The target project.
    pub project: i64,
}

/// What happens to a value on a move (FLD-059).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoveOutcome {
    /// The target has no such field: the value is gone.
    Erased,
    /// The target's set lacks it: what is left, the target's default or nothing.
    Replaced,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MoveChange {
    pub field: FieldRef,
    pub outcome: MoveOutcome,
    pub before: ChangeValues,
    pub after: ChangeValues,
}

/// What a move would do (ISS-025).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MovePreview {
    pub target: ProjectRef,
    pub changes: Vec<MoveChange>,
    /// The caller could not see the issue afterwards.
    pub loses_access: bool,
}

/// The issue after a move; `null` when the caller cannot see it there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MoveResult {
    #[schema(required = true)]
    pub issue: Option<Issue>,
    /// What the project's rules said (AUT-023, step 1.9).
    pub messages: Vec<crate::rules::Message>,
}

/// The issue after a change that can fire behaviors, with what they said (AUT-023, step 1.9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IssueAnswer {
    pub issue: Issue,
    pub messages: Vec<crate::rules::Message>,
}

/// Issues to clone as the caller's drafts (AUT-066).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CloneDraftsRequest {
    /// 1–50 internal IDs.
    pub issues: Vec<i64>,
}

/// The drafts made by cloning, in the order asked (AUT-066).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CloneDraftsAnswer {
    pub drafts: Vec<Issue>,
    pub messages: Vec<crate::rules::Message>,
}

/// How many issues the caller sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueCount {
    pub all: i64,
    pub unresolved: i64,
}

/// Whom the audience picker offers (ISS-043).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AudienceCandidates {
    /// The project's team first.
    pub groups: Vec<GroupRef>,
    pub users: Vec<UserRef>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CandidateQuery {
    pub q: Option<String>,
}

/// Kinds of history records (HIST-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    Created,
    Summary,
    Description,
    Field,
    Project,
    Tags,
    Visibility,
    Resolved,
    Reopened,
    Deleted,
    /// Files attached or deleted (CNT-017, CNT-024).
    Attachments,
    /// Links added or removed (HIST-005).
    Links,
    /// Added to or removed from a board (HIST-002).
    Boards,
}

/// Where a change came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryOrigin {
    Web,
    Api,
    /// Background work, such as applying an administrator's change of fields.
    System,
    /// A built-in behavior made the change; the author is who triggered it (AUT-023).
    Rule,
}

/// A value of a set named by history; its name is the one it had then.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LoggedValue {
    pub id: i64,
    pub name: String,
    /// Current color; `null` also when the value is gone.
    #[schema(required = true)]
    pub color: Option<i16>,
    /// The value no longer exists.
    pub deleted: bool,
}

/// An issue's place before or after a move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssuePlace {
    /// `null` when the caller may not see that project.
    #[schema(required = true)]
    pub project: Option<ProjectRef>,
    pub readable_id: String,
}

/// What left or came with a change. Only the members of the change's kind are filled: `texts`
/// for summaries, descriptions, strings, dates (`YYYY-MM-DD`) and times (RFC 3339); `numbers`
/// for numbers and durations (minutes); `values`, `users`, `groups` for fields with values and
/// audiences; `tags`; `place` for moves; `files` for attachments; `links` for links (only those to
/// issues the caller may see).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ChangeValues {
    pub texts: Vec<String>,
    pub numbers: Vec<f64>,
    pub values: Vec<LoggedValue>,
    pub users: Vec<UserRef>,
    pub groups: Vec<GroupRef>,
    pub tags: Vec<TagRef>,
    #[schema(required = true)]
    pub place: Option<IssuePlace>,
    /// Files of an `attachments` record, with the names they had.
    pub files: Vec<LoggedFile>,
    /// Links of a `links` record.
    pub links: Vec<LoggedLink>,
    /// The board place of a `boards` record, with the names it had.
    pub boards: Vec<LoggedBoard>,
}

/// A board, and the sprint of a sprint board, named by history (names as they were).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LoggedBoard {
    pub board: LoggedFile,
    #[schema(required = true)]
    pub sprint: Option<LoggedFile>,
}

/// A file named by history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LoggedFile {
    pub id: i64,
    pub name: String,
}

/// One record of the change history (HIST-001…HIST-005).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HistoryItem {
    /// The record's ID: a cursor of the stream.
    pub id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub author: UserRef,
    pub origin: HistoryOrigin,
    /// Made without notifications.
    pub silent: bool,
    pub kind: HistoryKind,
    /// The field of a `field` record.
    #[schema(required = true)]
    pub field: Option<FieldRef>,
    pub removed: ChangeValues,
    pub added: ChangeValues,
    /// The comment this change is shown under: its author changed the issue within a minute
    /// after posting it (HIST-011).
    #[schema(required = true)]
    pub after_comment: Option<i64>,
    /// The original this `created` record names, when the issue was made by cloning (AUT-066).
    #[serde(default)]
    #[schema(required = true)]
    pub clone_of: Option<CloneOfRef>,
}

/// The original issue a clone's `created` record names (AUT-066).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CloneOfRef {
    pub id: i64,
    /// Whether the viewer may open the original.
    pub readable: bool,
    /// The original as the viewer may see it; `null` when the viewer may not read it.
    #[schema(required = true)]
    pub issue: Option<IssueBrief>,
}

/// A comment in the stream, at its posting (HIST-006).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommentItem {
    /// The record's ID: a cursor of the stream.
    pub id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub comment: Comment,
}

/// One item of an issue's activity stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActivityItem {
    Change(Box<HistoryItem>),
    Comment(Box<CommentItem>),
}

/// Which way an activity page goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityDirection {
    /// Older items, newest first.
    #[default]
    Older,
    /// Newer items, oldest first.
    Newer,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ActivityQuery {
    /// The item to start after (excluded); omit for the newest (`older`) or oldest (`newer`).
    pub from: Option<i64>,
    #[param(inline)]
    pub direction: Option<ActivityDirection>,
    /// Items per page, 1–100; default 50.
    pub limit: Option<u32>,
    /// Show comments; default `true` (HIST-008).
    pub comments: Option<bool>,
    /// Show change records; default `true` (HIST-008).
    pub changes: Option<bool>,
    /// The page around this comment instead: the comment, older and newer items, newest first
    /// (CNT-007). `from` and `direction` are ignored.
    pub around_comment: Option<i64>,
}

/// A page of the activity stream (HIST-001, HIST-010).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ActivityPage {
    /// Newest first, except pages going `newer`, which are oldest first.
    pub items: Vec<ActivityItem>,
    /// Pass as `from` with `older` for older items; `null` when there are none.
    #[schema(required = true)]
    pub older: Option<i64>,
    /// Pass as `from` with `newer` for newer items; `null` when there are none.
    #[schema(required = true)]
    pub newer: Option<i64>,
}

/// What the issue page shows on opening, in one answer: the issue, the first page of its
/// activity (by the `ActivityQuery` parameters), its links and its files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IssuePage {
    pub issue: Issue,
    pub activity: ActivityPage,
    pub links: crate::links::IssueLinks,
    pub attachments: crate::files::AttachmentPage,
    /// The boards and sprints holding it (BRD-033).
    pub boards: Vec<crate::boards::IssueBoard>,
}

/// Whom the mention list offers (CNT-054).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MentionCandidates {
    pub users: Vec<UserRef>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MentionQuery {
    /// The start of a name or username, as typed after `@`.
    pub q: Option<String>,
    /// 1–20; default 10.
    pub limit: Option<u32>,
}

/// Issues to read at once (`POST /issues/batch`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueBatchRequest {
    /// 1–100 internal IDs.
    pub ids: Vec<i64>,
}

/// Issues read at once: in the order asked, without those missing or not readable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IssueBatch {
    pub items: Vec<Issue>,
}

/// What a field of an issue may hold for the caller (FLD-001…FLD-014). Only the member of the
/// field's kind is filled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FieldOptions {
    /// Values of the field's set in its order, without archived ones.
    pub values: Vec<OptionValue>,
    /// People of a user field's set by name, the caller first when they qualify.
    pub users: Vec<UserRef>,
    /// Groups of a group field.
    pub groups: Vec<GroupRef>,
    /// More options match than were answered.
    pub more: bool,
    /// The caller may add a value to the field's set from the picker.
    pub can_add_value: bool,
    /// The field's set, where a value added from the picker goes (`POST /value-sets/{id}/values`).
    #[schema(required = true)]
    pub set: Option<i64>,
}

/// A value as a picker offers it: what an issue holds, and for versions their release (FLD-047).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OptionValue {
    pub id: i64,
    pub name: String,
    #[schema(required = true)]
    pub color: Option<i16>,
    /// Shown as a hint under the option (FLD-043).
    pub description: String,
    pub archived: bool,
    pub resolved: bool,
    /// Versions: released.
    pub released: bool,
    /// Versions: the release date.
    #[schema(required = true)]
    pub release_date: Option<Date>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FieldOptionsQuery {
    /// Part of a name, username or value name.
    pub q: Option<String>,
    /// 1–50; default 20.
    pub limit: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_read_by_shape() {
        let read = |text: &str| serde_json::from_str::<FieldValueInput>(text).unwrap();
        assert_eq!(read("5"), FieldValueInput::Integer(5));
        assert_eq!(read("2.5"), FieldValueInput::Float(2.5));
        assert_eq!(read(r#""1h""#), FieldValueInput::Text("1h".to_owned()));
        assert_eq!(read("[1, 2]"), FieldValueInput::Ids(vec![1, 2]));
        let change: FieldValueChange = serde_json::from_str(r#"{"value": null}"#).unwrap();
        assert_eq!(change.value, None);
        assert!(!change.silent);
    }
}
