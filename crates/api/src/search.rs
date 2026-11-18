//! Searching issues with the query language (QRY): what `GET /issues`, the counts and the query
//! check take and answer. Offsets in queries are UTF-16 code units, as JavaScript counts them.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::issues::Issue;

/// The parameters of a search; every one is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct IssueSearchQuery {
    /// The query, such as `assignee: me is: unresolved sort: priority`; empty or absent is every
    /// issue the caller may see.
    pub q: Option<String>,
    /// Search within this project: its issues, its field names.
    pub project: Option<i64>,
    /// Search among the issues with this tag.
    pub tag: Option<i64>,
    /// Search among the issues of this saved search, as the caller sees them. When its own query
    /// fails, the answer is `invalid_query` without places in `q`; the saved search itself shows
    /// where.
    pub saved_search: Option<i64>,
    /// Leave resolved issues out unless the query itself is about resolution.
    pub hide_resolved: Option<bool>,
    /// IANA zone that dates such as `today` are read in; the instance's zone by default.
    pub time_zone: Option<String>,
    /// Issues per page, 1…100; default 50.
    pub limit: Option<u32>,
    /// `next_cursor` from the previous page of the same query.
    pub cursor: Option<String>,
}

/// One query of a batch of counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub project: Option<i64>,
    #[serde(default)]
    pub tag: Option<i64>,
    #[serde(default)]
    pub saved_search: Option<i64>,
    #[serde(default)]
    pub time_zone: Option<String>,
}

/// Up to 50 queries to count at once, such as the lists of a sidebar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountsRequest {
    pub queries: Vec<CountQuery>,
}

/// The counts in the order of the queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountsAnswer {
    pub items: Vec<CountItem>,
}

/// The count of one query, or why it could not run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountItem {
    #[schema(required = true)]
    pub all: Option<i64>,
    #[schema(required = true)]
    pub unresolved: Option<i64>,
    /// Empty when the query ran.
    pub problems: Vec<QueryProblem>,
}

/// A page of issues matching a query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IssueSearchPage {
    pub items: Vec<Issue>,
    /// Pass as `cursor` with the same query for the next page; `null` on the last page.
    #[schema(required = true)]
    pub next_cursor: Option<String>,
    /// The order used, ending with the tie-breaker.
    pub sort: Vec<SortUsed>,
    /// Notes about the query, such as a toggle it overrides.
    pub notes: Vec<QueryProblem>,
    /// The query written canonically.
    pub query: String,
    /// The query is one issue ID: a client may open that issue instead of a list.
    #[schema(required = true)]
    pub single_id: Option<i64>,
    /// The live topic of this list: subscribe to it to hear about changes of issues this list
    /// can contain (see QRY-086).
    pub live_topic: String,
}

/// One key of the order of a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SortUsed {
    pub key: SortKeyName,
    /// The field, for `key = field`.
    #[schema(required = true)]
    pub field: Option<i64>,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortKeyName {
    Updated,
    Created,
    Resolved,
    Id,
    Votes,
    Comments,
    Project,
    Reporter,
    Summary,
    Starred,
    Field,
    /// How well the issue matches the query's text (QRY-045): the order of a query with text
    /// and no `sort`.
    Relevance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// A problem of a query at its place in the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QueryProblem {
    pub code: QueryProblemCode,
    /// Start, in UTF-16 code units.
    pub from: u32,
    /// End (exclusive), in UTF-16 code units.
    pub to: u32,
    pub severity: QuerySeverity,
}

/// An error stops the query; a note only informs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuerySeverity {
    Error,
    Note,
}

/// Every problem a query can have. Closed: clients translate each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryProblemCode {
    UnclosedQuote,
    UnclosedGroup,
    UnexpectedClose,
    UnexpectedSymbol,
    MissingValue,
    MissingOperand,
    EmptyGroup,
    SortTwice,
    SortInGroup,
    BadRange,
    TooDeep,
    TooManyTerms,
    QueryTooLong,
    UnknownAttribute,
    UnknownValue,
    AmbiguousUser,
    WrongOperator,
    BadDate,
    BadNumber,
    NotSortable,
    TooManySortKeys,
    DeletedReference,
    SameFieldTwice,
    FieldShadowed,
    HideResolvedIgnored,
    IgnoredCommonWord,
}

/// The parameters of a query check.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueryCheckQuery {
    pub q: Option<String>,
    /// Names are read as within this project.
    pub project: Option<i64>,
    pub time_zone: Option<String>,
}

/// How a query reads, for highlighting while it is typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QueryCheck {
    pub tokens: Vec<QueryToken>,
    /// Errors and notes, in text order.
    pub problems: Vec<QueryProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QueryToken {
    pub from: u32,
    pub to: u32,
    pub class: TokenClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TokenClass {
    Attribute,
    Value,
    Keyword,
    Operator,
    Text,
    Punctuation,
    Error,
}

/// The parameters of a query assist (QRY-056).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueryAssistQuery {
    pub q: Option<String>,
    /// The caret, in UTF-16 code units, as JavaScript counts it.
    pub caret: Option<u32>,
    /// Names are read as within this project.
    pub project: Option<i64>,
    pub time_zone: Option<String>,
}

/// Completion for the query bar (QRY-056…QRY-058): the check — tokens and problems for the
/// highlighting and markers — plus the suggestions at the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QueryAssist {
    pub tokens: Vec<QueryToken>,
    pub problems: Vec<QueryProblem>,
    pub suggestions: Vec<crate::commands::Suggestion>,
}
