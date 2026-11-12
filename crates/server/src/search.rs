//! Searching issues with the query language (QRY): `GET /issues`, the counts and the query check.
//! Rules live in `trackflow_service::search`; here byte spans become UTF-16 offsets.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use trackflow_api::error::ErrorBody;
use trackflow_api::fields::{IssueFields, IssueFieldsQuery};
use trackflow_api::issues::{Issue, IssueCount};
use trackflow_api::search::{
    CountItem, CountsAnswer, CountsRequest, IssueSearchPage, IssueSearchQuery, QueryCheck,
    QueryCheckQuery, QueryProblem, QueryProblemCode, QuerySeverity, QueryToken, SortDirection,
    SortKeyName, SortUsed, TokenClass,
};
use trackflow_domain::auth::Scope;
use trackflow_domain::search::{SortBy, SortSpec};
use trackflow_query::{Class, Direction, Problem, ProblemCode, Severity, Span, utf16};
use trackflow_service::Error;
use trackflow_service::search::{self, SearchInput};

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::fields::field_ref;
use crate::issues::issue;

const DEFAULT_LIMIT: u32 = 50;

/// Fields of the projects the caller reads issues in, or of one project (QRY-078): the columns an
/// issue list may show. Private fields only where the caller reads them (FLD-062).
#[utoipa::path(
    get,
    path = "/issues/fields",
    operation_id = "list_issue_fields",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(IssueFieldsQuery),
    responses(
        (status = OK, description = "The fields, most used first", body = IssueFields),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn fields(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<IssueFieldsQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found =
        trackflow_service::fields::of_readable(&state.db, &viewer.access, query.project).await?;
    Ok(json(
        StatusCode::OK,
        IssueFields {
            items: found.iter().map(|(f, _)| field_ref(f)).collect(),
        },
    ))
}

/// Issues matching a query, in its order (QRY-001…QRY-092). Without `q` every issue the caller
/// may see, most recently updated first; `project` alone lists that project.
#[utoipa::path(
    get,
    path = "/issues",
    operation_id = "search_issues",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(IssueSearchQuery),
    responses(
        (status = OK, description = "A page of issues", body = IssueSearchPage),
        (status = BAD_REQUEST, description = "The query has errors (`invalid_query`), or a bad limit, cursor or time zone", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project or tag", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<IssueSearchQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let text = query.q.clone().unwrap_or_default();
    let page = search::search(&state.db, &viewer.access, input(query))
        .await
        .map_err(|e| run_error(&text, e))?;
    Ok(json(
        StatusCode::OK,
        search_page(&state, &viewer.access, &text, page),
    ))
}

/// A page of issues on the wire; `text` is the query its notes point into.
pub(crate) fn search_page(
    state: &AppState,
    viewer: &trackflow_domain::access::Access,
    text: &str,
    page: search::SearchPage,
) -> IssueSearchPage {
    let names = state.names();
    let signer = state.signer();
    let d = &page.issues;
    let items: Vec<Issue> = d
        .issues
        .iter()
        .map(|v| issue(&names, &signer, viewer, d, v))
        .collect();
    IssueSearchPage {
        items,
        next_cursor: page.next_cursor,
        sort: page.sort.iter().map(sort_used).collect(),
        notes: problems(text, &page.notes),
        query: page.canonical,
        single_id: page.single_id,
        live_topic: page.live_topic,
    }
}

/// How many issues match a query: all and unresolved (QRY-092).
#[utoipa::path(
    get,
    path = "/issues/count",
    operation_id = "count_issues",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(IssueSearchQuery),
    responses(
        (status = OK, description = "Counts", body = IssueCount),
        (status = BAD_REQUEST, description = "The query has errors", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project or tag", body = ErrorBody),
    ),
)]
pub async fn count(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<IssueSearchQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let text = query.q.clone().unwrap_or_default();
    let (all, unresolved) = search::count(&state.db, &viewer.access, input(query))
        .await
        .map_err(|e| run_error(&text, e))?;
    Ok(json(StatusCode::OK, IssueCount { all, unresolved }))
}

/// Counts of up to 50 queries at once (QRY-092). A read with a body: the queries do not fit a URL.
/// An invalid query gets its problems in its place; the others still count.
#[utoipa::path(
    post,
    path = "/issues/counts",
    operation_id = "count_issue_queries",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = CountsRequest,
    responses(
        (status = OK, description = "Counts in the order of the queries", body = CountsAnswer),
        (status = BAD_REQUEST, description = "More than 50 queries", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project or tag", body = ErrorBody),
    ),
)]
pub async fn counts(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<CountsRequest>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let texts: Vec<String> = request.queries.iter().map(|q| q.q.clone()).collect();
    let inputs = request
        .queries
        .into_iter()
        .map(|q| SearchInput {
            text: q.q,
            project: q.project,
            tag: q.tag,
            saved_search: q.saved_search,
            time_zone: q.time_zone,
            limit: 1,
            ..SearchInput::default()
        })
        .collect();
    let answers = search::counts(&state.db, &viewer.access, inputs).await?;
    let items = answers
        .into_iter()
        .zip(&texts)
        .map(|(answer, text)| match answer {
            Ok((all, unresolved)) => CountItem {
                all: Some(all),
                unresolved: Some(unresolved),
                problems: Vec::new(),
            },
            Err(found) => CountItem {
                all: None,
                unresolved: None,
                problems: problems(text, &found),
            },
        })
        .collect();
    Ok(json(StatusCode::OK, CountsAnswer { items }))
}

/// How a query reads while it is typed: a class for each token and the problems, with names
/// resolved for the caller (the server half of highlighting).
#[utoipa::path(
    get,
    path = "/queries/check",
    operation_id = "check_query",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(QueryCheckQuery),
    responses(
        (status = OK, description = "Tokens and problems", body = QueryCheck),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn check(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<QueryCheckQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let text = query.q.unwrap_or_default();
    let found = search::check(
        &state.db,
        &viewer.access,
        &text,
        query.project,
        query.time_zone,
    )
    .await?;
    let spans: Vec<Span> = found.tokens.iter().map(|(span, _)| *span).collect();
    let tokens = utf16::offsets(&text, &spans)
        .into_iter()
        .zip(&found.tokens)
        .map(|(span, (_, class))| QueryToken {
            from: offset(span.start),
            to: offset(span.end),
            class: token_class(*class),
        })
        .collect();
    Ok(json(
        StatusCode::OK,
        QueryCheck {
            tokens,
            problems: problems(&text, &found.problems),
        },
    ))
}

fn input(query: IssueSearchQuery) -> SearchInput {
    SearchInput {
        text: query.q.unwrap_or_default(),
        project: query.project,
        tag: query.tag,
        saved_search: query.saved_search,
        hide_resolved: query.hide_resolved.unwrap_or(false),
        time_zone: query.time_zone,
        cursor: query.cursor,
        limit: query.limit.unwrap_or(DEFAULT_LIMIT),
        now: None,
        manual_order: None,
    }
}

/// An invalid query answers with its problems at their places in `text`.
pub(crate) fn run_error(text: &str, error: Error) -> ApiError {
    match error {
        Error::InvalidQuery(found) => ApiError::invalid_query(problems(text, &found)),
        other => other.into(),
    }
}

/// Problems of `text` with UTF-16 offsets.
pub(crate) fn problems(text: &str, found: &[Problem]) -> Vec<QueryProblem> {
    let spans: Vec<Span> = found.iter().map(|p| p.span).collect();
    utf16::offsets(text, &spans)
        .into_iter()
        .zip(found)
        .map(|(span, p)| QueryProblem {
            code: problem_code(p.code),
            from: offset(span.start),
            to: offset(span.end),
            severity: match p.severity {
                Severity::Error => QuerySeverity::Error,
                Severity::Note => QuerySeverity::Note,
            },
        })
        .collect()
}

fn offset(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

pub(crate) fn sort_used(spec: &SortSpec) -> SortUsed {
    let (key, field) = match &spec.by {
        SortBy::Updated => (SortKeyName::Updated, None),
        SortBy::Created => (SortKeyName::Created, None),
        SortBy::Resolved => (SortKeyName::Resolved, None),
        SortBy::Id => (SortKeyName::Id, None),
        SortBy::Votes => (SortKeyName::Votes, None),
        SortBy::Comments => (SortKeyName::Comments, None),
        SortBy::Project => (SortKeyName::Project, None),
        SortBy::Reporter => (SortKeyName::Reporter, None),
        SortBy::Summary => (SortKeyName::Summary, None),
        SortBy::Starred => (SortKeyName::Starred, None),
        SortBy::Relevance(_) => (SortKeyName::Relevance, None),
        SortBy::Positions { field, .. }
        | SortBy::Users { field }
        | SortBy::Groups { field }
        | SortBy::Scalar { field } => (SortKeyName::Field, Some(field.id)),
    };
    SortUsed {
        key,
        field,
        direction: match spec.direction {
            Direction::Asc => SortDirection::Asc,
            Direction::Desc => SortDirection::Desc,
        },
    }
}

fn token_class(class: Class) -> TokenClass {
    match class {
        Class::Attribute => TokenClass::Attribute,
        Class::Value => TokenClass::Value,
        Class::Keyword => TokenClass::Keyword,
        Class::Operator => TokenClass::Operator,
        Class::Text => TokenClass::Text,
        Class::Punctuation => TokenClass::Punctuation,
        Class::Error => TokenClass::Error,
    }
}

fn problem_code(code: ProblemCode) -> QueryProblemCode {
    use QueryProblemCode as Q;
    match code {
        ProblemCode::UnclosedQuote => Q::UnclosedQuote,
        ProblemCode::UnclosedGroup => Q::UnclosedGroup,
        ProblemCode::UnexpectedClose => Q::UnexpectedClose,
        ProblemCode::UnexpectedSymbol => Q::UnexpectedSymbol,
        ProblemCode::MissingValue => Q::MissingValue,
        ProblemCode::MissingOperand => Q::MissingOperand,
        ProblemCode::EmptyGroup => Q::EmptyGroup,
        ProblemCode::SortTwice => Q::SortTwice,
        ProblemCode::SortInGroup => Q::SortInGroup,
        ProblemCode::BadRange => Q::BadRange,
        ProblemCode::TooDeep => Q::TooDeep,
        ProblemCode::TooManyTerms => Q::TooManyTerms,
        ProblemCode::QueryTooLong => Q::QueryTooLong,
        ProblemCode::UnknownAttribute => Q::UnknownAttribute,
        ProblemCode::UnknownValue => Q::UnknownValue,
        ProblemCode::AmbiguousUser => Q::AmbiguousUser,
        ProblemCode::WrongOperator => Q::WrongOperator,
        ProblemCode::BadDate => Q::BadDate,
        ProblemCode::BadNumber => Q::BadNumber,
        ProblemCode::NotSortable => Q::NotSortable,
        ProblemCode::TooManySortKeys => Q::TooManySortKeys,
        ProblemCode::DeletedReference => Q::DeletedReference,
        ProblemCode::SameFieldTwice => Q::SameFieldTwice,
        ProblemCode::FieldShadowed => Q::FieldShadowed,
        ProblemCode::HideResolvedIgnored => Q::HideResolvedIgnored,
        ProblemCode::IgnoredCommonWord => Q::IgnoredCommonWord,
    }
}
