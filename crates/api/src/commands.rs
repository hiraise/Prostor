//! The command language (step 2.1, `CMD`): what the command endpoints take and answer. A
//! command line's own failures ride the answer as `cmd_*` codes over ranges of the text, never
//! HTTP errors.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::issues::AudienceInput;
use crate::rules::Message;

/// One command line to apply (plan §4.8) to at most 50 named issues — a query's worth goes
/// through the bulk job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApplyRequest {
    pub text: String,
    /// The issues to apply to, by their internal IDs.
    pub issues: Vec<i64>,
    /// Markdown the `comment` command attaches (the dialog's comment field).
    #[schema(required = true)]
    pub comment: Option<String>,
    /// The comment's audience; `null` is the issue's own.
    #[schema(required = true)]
    pub comment_visibility: Option<AudienceInput>,
    /// Without subscriber notifications; needs `issue.silent` in every affected project.
    #[serde(default)]
    pub silent: bool,
    /// Make the create-value quick fixes before resolving the plan again.
    #[serde(default)]
    pub create_missing: bool,
    /// IANA name; the instance's `default_time_zone` when `null`.
    #[schema(required = true)]
    pub time_zone: Option<String>,
    /// Apply as another account: `system.write`, audited (CMD-031).
    #[schema(required = true)]
    pub run_as: Option<i64>,
}

/// One problem over a range of the command text (plan §4.2): a `cmd_*` code, where the line
/// says it, and what the code names — the fields a bare value fits, the permission lacked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommandProblem {
    /// `cmd_unknown_word`, `cmd_value_expected`, `cmd_unknown_value`, `cmd_ambiguous`,
    /// `cmd_not_empty`, `cmd_wrong_kind`, `cmd_text_field`, `cmd_own_issue_vote`,
    /// `cmd_no_permission`, `cmd_target_not_found`, `cmd_board_no_sprints`.
    pub code: String,
    /// Where the problem sits in the text, in bytes.
    pub start: usize,
    pub end: usize,
    pub params: BTreeMap<String, String>,
    /// Create the value the line names and the field lacks (plan §5); the dialog offers it,
    /// the apply's `create_missing` makes it.
    #[serde(default)]
    #[schema(required = true)]
    pub quick_fix: Option<CommandQuickFix>,
}

/// The fix for a value the command names and the field lacks (plan §5): create it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommandQuickFix {
    /// The name as typed, trimmed: the value's title.
    pub name: String,
    /// The field the value joins.
    pub field: i64,
    /// The projects sharing the set — those a creation updates.
    pub projects: Vec<i64>,
}

/// One planned operation over its range (plan §4.2): what the preview shows a row for. The
/// assist of step 2.1b fills the names; this is the shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PlannedOperation {
    /// `set_field`, `tag`, `move`, `link`, `duplicate`, `visibility`, `board`, `star`, `vote`,
    /// `comment`, `delete`, `clone`.
    pub kind: String,
    /// Where the command sits in the text, in bytes.
    pub start: usize,
    pub end: usize,
    /// What the operation names, as it reads (plan §5): the field and its values, the tag,
    /// the project, the link's direction and target, the party, the board and sprint. The
    /// preview's rows build on these; empty for the nameless kinds.
    #[serde(default)]
    pub names: Vec<String>,
    /// `set`, `add` or `remove` — the direction the operation changes things in; `null` where
    /// the kind says it (a `move`, a `comment`). The assist fills it for the preview's marks.
    #[serde(default)]
    #[schema(required = true)]
    pub mode: Option<String>,
    /// The values a plain set on a multi-value field takes off every target issue (plan §1:
    /// «Fix versions 2025.3» leaves only 2025.3), named as they read. The assist fills it; a
    /// bulk query, whose targets vary, leaves it empty.
    #[serde(default)]
    pub removals: Vec<String>,
}

/// Why one issue's apply failed: the same codes the plan uses, with the detail the web says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueProblem {
    pub code: String,
    pub detail: String,
}

/// One issue's result (plan §4.3): applied whole, or not at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IssueResult {
    pub issue: i64,
    pub ok: bool,
    #[schema(required = true)]
    pub problem: Option<IssueProblem>,
    /// What the project's behaviors said; the web turns each into a toast.
    pub messages: Vec<Message>,
}

/// The answer of `POST /commands` (plan §4.8): the plan the line resolved to — after any quick
/// fixes — and one result per issue, in the request's order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ApplyAnswer {
    pub operations: Vec<PlannedOperation>,
    /// Present: the apply touched nothing and the dialog shows each problem.
    pub problems: Vec<CommandProblem>,
    pub issues: Vec<IssueResult>,
}

/// A bulk apply over a query (plan §4.7): enqueues the job that walks the query's issues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BulkRequest {
    pub text: String,
    /// The query every matching issue of which the line applies to; empty means the named
    /// project's every issue.
    pub query: String,
    #[schema(required = true)]
    pub project: Option<i64>,
    #[serde(default)]
    pub hide_resolved: bool,
    #[schema(required = true)]
    pub time_zone: Option<String>,
    #[schema(required = true)]
    pub comment: Option<String>,
    #[schema(required = true)]
    pub comment_visibility: Option<AudienceInput>,
    #[serde(default)]
    pub silent: bool,
    #[serde(default)]
    pub create_missing: bool,
    #[schema(required = true)]
    pub run_as: Option<i64>,
}

/// The queued bulk job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BulkQueued {
    pub job: i64,
}

/// One issue the bulk job could not apply the line to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BulkFailure {
    pub issue: i64,
    pub code: String,
    pub detail: String,
}

/// The bulk job's progress and results (plan §4.7), for its owner alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BulkProgress {
    pub id: i64,
    /// `queued`, `running`, `finished`, `cancelled`, `failed`.
    pub state: String,
    pub done: i64,
    #[schema(required = true)]
    pub total: Option<i64>,
    pub failed: i64,
    pub failures: Vec<BulkFailure>,
    pub cancel_requested: bool,
    #[schema(required = true)]
    pub last_error: Option<String>,
}

/// One command line the dialog is typing (plan §5): the text, the caret, and the targets the
/// line would apply to — named issues, or the bulk flow's query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AssistRequest {
    pub text: String,
    /// The caret, in UTF-16 code units, as JavaScript counts it.
    pub caret: u32,
    /// The issues the dialog applies to, by their internal IDs.
    #[serde(default)]
    pub issues: Vec<i64>,
    /// The query the bulk dialog applies the line to; empty means the project's every issue.
    #[schema(required = true)]
    pub query: Option<String>,
    #[schema(required = true)]
    pub project: Option<i64>,
    #[serde(default)]
    pub hide_resolved: bool,
    /// Markdown the `comment` command attaches; the preview says when it is missing.
    #[schema(required = true)]
    pub comment: Option<String>,
    /// IANA name; the instance's `default_time_zone` when `null`.
    #[schema(required = true)]
    pub time_zone: Option<String>,
}

/// The answer of `POST /commands/assist` (plan §5): the parsed plan for the preview, its
/// problems over ranges with their quick fixes, and the completion at the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommandAssist {
    pub operations: Vec<PlannedOperation>,
    pub problems: Vec<CommandProblem>,
    pub suggestions: Vec<Suggestion>,
}

/// One completion (QRY-056's contract, shared by both surfaces, D-061): what to insert, where,
/// and how it matched. Offsets are UTF-16 code units — `from` and `to` into the current text,
/// `matched_from` and `matched_to` into `text`, `caret` into the text after the insert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Suggestion {
    /// The insert, quoted where the plain word would not read back the same.
    pub text: String,
    /// Plain data the label builds on: the field of a value, a project's key, an account's
    /// full name, an issue's summary. Empty when the group says it all.
    pub description: String,
    pub group: SuggestionGroup,
    pub from: u32,
    pub to: u32,
    pub matched_from: u32,
    pub matched_to: u32,
    pub caret: u32,
    /// What follows the insert: a space, or the separator the position calls for.
    pub suffix: String,
    /// The value is already on every target issue (commands only); the web marks it.
    #[serde(default)]
    pub present: bool,
    /// A relative date's concrete value in the caller's zone, as `YYYY-MM-DD`.
    #[schema(required = true)]
    pub resolved: Option<String>,
    /// The actor's own account.
    #[serde(default)]
    pub own: bool,
}

/// What a suggestion is about; clients label and order by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionGroup {
    Command,
    Attribute,
    Value,
    Link,
    Tag,
    Project,
    User,
    Group,
    Board,
    Sprint,
    Special,
    Empty,
    Date,
    Issue,
    Recent,
    Keyword,
    SortKey,
    SortDirection,
}
