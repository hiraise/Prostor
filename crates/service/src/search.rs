//! Searching issues (QRY-*, plan `docs/plans/step-1.5.md` §3–§4): a query's text is parsed
//! (`taskmaster_query`), its names resolved against a catalog loaded for this viewer
//! (`domain::search`), and the result compiled to SQL (`store::search`).
//!
//! A query with an error does not run (`Error::InvalidQuery` lists every problem with its span):
//! an unknown name is never taken for text, so nobody gets silently wrong results.

use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::access::{Access, Projects};
use taskmaster_domain::search::{
    self as domain, Binding, Catalog, Clock, Ctx, Filter, SortSpec, Stored,
};
use taskmaster_domain::validation::Problem as InputProblem;
use taskmaster_domain::{BoardId, IssueId, ProjectId, TagId};
use taskmaster_query::{Class, Problem, ProblemCode, Query, Severity, Span};
use taskmaster_store::saved_searches::SavedSearchId;
use taskmaster_store::search::catalog::{self, ProjectRow};
use taskmaster_store::search::{self as store, SortValue};
use taskmaster_store::visibility::{Visibility, holds};
use taskmaster_store::{PgPool, issues as store_issues, settings, tags};
use time::OffsetDateTime;

use crate::issues::{Described, describe};
use crate::{Error, Result};

/// Most issues on one page.
pub const LIMIT_MAX: u32 = 100;
/// Most queries in one batch of counts.
pub const COUNTS_MAX: usize = 50;

/// One search.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchInput {
    pub text: String,
    /// Search within this project (QRY-053): its names, its issues.
    pub project: Option<ProjectId>,
    /// Search among issues with this tag (QRY-053).
    pub tag: Option<TagId>,
    /// Search among the issues of this saved search (QRY-053), as the viewer sees them (QRY-069).
    pub saved_search: Option<SavedSearchId>,
    /// Leave resolved issues out unless the query is about resolution (QRY-054).
    pub hide_resolved: bool,
    /// IANA name; the instance's `default_time_zone` when `None`.
    pub time_zone: Option<String>,
    /// From the previous page's `next_cursor`.
    pub cursor: Option<String>,
    /// 1…[`LIMIT_MAX`].
    pub limit: u32,
    /// The clock dates are read with; `None` is now. Tests fix it.
    pub now: Option<OffsetDateTime>,
    /// The manual order of this saved search comes first (BRD-093, a backlog): the issues it
    /// ranks by rank, then the rest by the query's sort.
    pub manual_order: Option<SavedSearchId>,
}

#[derive(Debug, Clone)]
pub struct SearchPage {
    pub issues: Described,
    pub next_cursor: Option<String>,
    /// Notes about the query (`hide_resolved_ignored`, `same_field_twice`…).
    pub notes: Vec<Problem>,
    /// The effective sort, ending with the tie-breaker.
    pub sort: Vec<SortSpec>,
    /// The query written canonically.
    pub canonical: String,
    /// The query is one issue ID (QRY-016): the UI opens it.
    pub single_id: Option<IssueId>,
    /// The live topic that hears about changes of issues this list can contain (QRY-086).
    pub live_topic: String,
}

/// Highlighting from the server's side: syntax classes refined by name resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub tokens: Vec<(Span, Class)>,
    /// Errors and notes, in text order.
    pub problems: Vec<Problem>,
}

/// A query ready to run.
pub(crate) struct Prepared {
    pub(crate) filter: Filter,
    sort: Vec<SortSpec>,
    pub(crate) vis: Visibility,
    notes: Vec<Problem>,
    canonical: String,
    single_id: Option<IssueId>,
    concerns_resolution: bool,
    bindings: Vec<(Span, Binding)>,
    /// Where the matches can be: the projects and whether the query chose them.
    scope: domain::Scope,
}

/// A page of issues matching the query, in its sort order.
pub async fn search(pool: &PgPool, viewer: &Access, input: SearchInput) -> Result<SearchPage> {
    if input.limit == 0 || input.limit > LIMIT_MAX {
        return Err(Error::field(
            "limit",
            InputProblem::OutOfRange,
            format!("limit must be between 1 and {LIMIT_MAX}"),
        ));
    }
    let mut conn = pool.acquire().await?;
    let prepared = prepare(&mut conn, viewer, &input).await?;
    let hash = fingerprint(&prepared.canonical, &input);
    let after = input
        .cursor
        .as_deref()
        .map(|c| read_cursor(c, &hash, prepared.sort.len(), input.manual_order.is_some()))
        .transpose()?;
    let limit = i64::from(input.limit);
    // Phase one: the ranked issues of the manual order; phase two: the rest by the sort.
    let mut ranked = Vec::new();
    let mut after_keys = None;
    match (input.manual_order, &after) {
        (Some(search), None | Some(At::Ranked(..))) => {
            let from = match &after {
                Some(At::Ranked(rank, id)) => Some((rank.as_str(), *id)),
                _ => None,
            };
            ranked = store::page_ranked(
                &mut conn,
                &prepared.filter,
                &prepared.vis,
                search,
                from,
                limit + 1,
            )
            .await?;
        }
        (_, Some(At::Sorted(keys))) => after_keys = Some(keys.as_slice()),
        (None, _) => {}
    }
    let room = limit + 1 - i64::try_from(ranked.len()).unwrap_or(limit + 1);
    let hits = if room > 0 {
        store::page(
            &mut conn,
            &prepared.filter,
            &prepared.sort,
            &prepared.vis,
            after_keys,
            input.manual_order,
            room,
        )
        .await?
    } else {
        Vec::new()
    };
    let mut items: Vec<(IssueId, At)> = ranked
        .into_iter()
        .map(|(id, rank)| (id, At::Ranked(rank, id)))
        .chain(hits.into_iter().map(|h| (h.id, At::Sorted(h.keys))))
        .collect();
    let next_cursor = if items.len() > input.limit as usize {
        items.truncate(input.limit as usize);
        items.last().map(|(_, at)| write_cursor(&hash, at))
    } else {
        None
    };
    let hits: Vec<IssueId> = items.iter().map(|(id, _)| *id).collect();
    let ids = hits;
    let mut found = store_issues::get_many(&mut *conn, &ids).await?;
    found.sort_by_key(|i| ids.iter().position(|id| *id == i.id));
    let issues = describe(&mut conn, viewer, found).await?;
    Ok(SearchPage {
        issues,
        next_cursor,
        notes: prepared.notes,
        sort: prepared.sort,
        live_topic: taskmaster_domain::live::list_topic(
            &prepared.scope.projects,
            prepared.scope.narrowed,
        )
        .to_string(),
        canonical: prepared.canonical,
        single_id: prepared.single_id,
    })
}

/// How many issues match: all and unresolved.
pub async fn count(pool: &PgPool, viewer: &Access, input: SearchInput) -> Result<(i64, i64)> {
    let mut conn = pool.acquire().await?;
    let prepared = prepare(&mut conn, viewer, &input).await?;
    Ok(store::count(&mut conn, &prepared.filter, &prepared.vis).await?)
}

/// The first `limit` issues a query matches, in its order, and how many it matches in all: what
/// an export takes (QRY-084). Unlike a page, `limit` may exceed [`LIMIT_MAX`]; a manual board
/// order (BRD-093) is not followed — exports come from lists, which have none.
pub async fn matching(
    conn: &mut PgConnection,
    viewer: &Access,
    input: SearchInput,
    limit: u32,
) -> Result<(Vec<IssueId>, i64)> {
    let prepared = prepare(conn, viewer, &input).await?;
    let total = store::count(conn, &prepared.filter, &prepared.vis).await?.0;
    let hits = store::page(
        conn,
        &prepared.filter,
        &prepared.sort,
        &prepared.vis,
        None,
        None,
        i64::from(limit),
    )
    .await?;
    Ok((hits.into_iter().map(|h| h.id).collect(), total))
}

/// Counts of up to [`COUNTS_MAX`] queries on one connection; an invalid query gets its problems
/// in its place.
pub async fn counts(
    pool: &PgPool,
    viewer: &Access,
    inputs: Vec<SearchInput>,
) -> Result<Vec<Result<(i64, i64), Vec<Problem>>>> {
    if inputs.len() > COUNTS_MAX {
        return Err(Error::field(
            "queries",
            InputProblem::OutOfRange,
            format!("at most {COUNTS_MAX} queries"),
        ));
    }
    let mut conn = pool.acquire().await?;
    let mut out = Vec::with_capacity(inputs.len());
    for input in &inputs {
        match prepare(&mut conn, viewer, input).await {
            Ok(prepared) => {
                out.push(Ok(
                    store::count(&mut conn, &prepared.filter, &prepared.vis).await?
                ));
            }
            Err(Error::InvalidQuery(problems)) => out.push(Err(problems)),
            Err(other) => return Err(other),
        }
    }
    Ok(out)
}

/// Classes and problems of a query's text, for highlighting while it is typed.
pub async fn check(
    pool: &PgPool,
    viewer: &Access,
    text: &str,
    project: Option<ProjectId>,
    time_zone: Option<String>,
) -> Result<Check> {
    let mut conn = pool.acquire().await?;
    let input = SearchInput {
        text: text.to_owned(),
        project,
        time_zone,
        ..SearchInput::default()
    };
    let mut q = taskmaster_query::parse(text);
    let problems = match prepare_parsed(&mut conn, viewer, &input, &q).await {
        Ok(prepared) => prepared.notes,
        Err(Error::InvalidQuery(problems)) => problems,
        Err(other) => return Err(other),
    };
    q.problems.clone_from(&problems);
    Ok(Check {
        tokens: taskmaster_query::classes(&q, text),
        problems,
    })
}

async fn prepare(
    conn: &mut PgConnection,
    viewer: &Access,
    input: &SearchInput,
) -> Result<Prepared> {
    let q = taskmaster_query::parse(&input.text);
    if q.problems.iter().any(|p| p.severity == Severity::Error) {
        return Err(Error::InvalidQuery(q.problems));
    }
    prepare_parsed(conn, viewer, input, &q).await
}

/// A query in the context of a saved search: both resolved by the viewer's names and rights,
/// ANDed; the query's sort, else the saved one's; "hide resolved" unless either is about
/// resolution.
async fn prepare_parsed(
    conn: &mut PgConnection,
    viewer: &Access,
    input: &SearchInput,
    q: &Query,
) -> Result<Prepared> {
    let Some(id) = input.saved_search else {
        let mut p = resolve_parsed(
            conn,
            viewer,
            input,
            q,
            input.hide_resolved,
            None,
            QueryBoards::Expand,
        )
        .await?;
        if p.vis.within.is_none() {
            p.filter = within_one(p.filter, &mut p.vis);
        }
        return Ok(p);
    };
    let saved = crate::saved_searches::query_for(conn, viewer, id).await?;
    let mut p = resolve_parsed(conn, viewer, input, q, false, None, QueryBoards::Expand).await?;
    let s = resolve_parsed(
        conn,
        viewer,
        input,
        &saved,
        false,
        None,
        QueryBoards::Expand,
    )
    .await?;
    let mut parts = vec![p.filter, s.filter];
    if input.hide_resolved && p.single_id.is_none() {
        if p.concerns_resolution || s.concerns_resolution {
            p.notes.push(Problem::new(
                ProblemCode::HideResolvedIgnored,
                Span::default(),
            ));
        } else {
            parts.push(Filter::Resolved(false));
        }
    }
    p.filter = Filter::and(parts);
    if q.sort.is_none() {
        p.sort = s.sort;
    }
    // Both must match, so either one's narrowing holds.
    p.scope = match (p.scope.narrowed, s.scope.narrowed) {
        (true, true) => domain::Scope {
            projects: (p.scope.projects.iter())
                .filter(|id| s.scope.projects.contains(id))
                .copied()
                .collect(),
            narrowed: true,
        },
        (false, true) => s.scope,
        _ => p.scope,
    };
    if p.vis.within.is_none() {
        p.filter = within_one(p.filter, &mut p.vis);
    }
    Ok(p)
}

/// Where a saved search's matches can be, for the viewer (a backlog's projects, BRD-090).
pub(crate) async fn saved_scope(
    conn: &mut PgConnection,
    viewer: &Access,
    id: SavedSearchId,
) -> Result<domain::Scope> {
    let q = crate::saved_searches::query_for(conn, viewer, id).await?;
    let p = resolve_parsed(
        conn,
        viewer,
        &SearchInput::default(),
        &q,
        false,
        None,
        QueryBoards::Expand,
    )
    .await?;
    Ok(p.scope)
}

/// The matches of a saved search its manual order does not rank, in its sort, up to and
/// including `issue`; `None` when `issue` is not among the first `max` (BRD-093).
pub(crate) async fn unranked_until(
    conn: &mut PgConnection,
    viewer: &Access,
    id: SavedSearchId,
    issue: IssueId,
    max: i64,
) -> Result<Option<Vec<IssueId>>> {
    let input = SearchInput {
        saved_search: Some(id),
        ..SearchInput::default()
    };
    let p = prepare_parsed(conn, viewer, &input, &taskmaster_query::parse("")).await?;
    let hits = store::page(conn, &p.filter, &p.sort, &p.vis, None, Some(id), max).await?;
    Ok(hits
        .iter()
        .position(|h| h.id == issue)
        .map(|n| hits[..=n].iter().map(|h| h.id).collect()))
}

/// The stored form of a query resolved with the viewer's names (QRY-063).
pub(crate) async fn bind(conn: &mut PgConnection, viewer: &Access, text: &str) -> Result<Stored> {
    let q = taskmaster_query::parse(text);
    if q.problems.iter().any(|p| p.severity == Severity::Error) {
        return Err(Error::InvalidQuery(q.problems));
    }
    let input = SearchInput {
        text: text.to_owned(),
        ..SearchInput::default()
    };
    let p = resolve_parsed(conn, viewer, &input, &q, false, None, QueryBoards::Expand).await?;
    Ok(Stored::new(&q, p.bindings))
}

/// The stored form of a query resolved within given projects (a board query, BRD-029).
pub(crate) async fn bind_within(
    conn: &mut PgConnection,
    viewer: &Access,
    text: &str,
    projects: &[ProjectId],
) -> Result<Stored> {
    let w = resolve_within(conn, viewer, text, projects, None, QueryBoards::Leave).await?;
    Ok(Stored::new(&w.query, w.bindings))
}

/// A query resolved within given projects (a board's: its header filter and its query, BRD-081,
/// BRD-029): their names, their issues.
#[derive(Debug, Clone)]
pub(crate) struct Within {
    pub filter: Filter,
    /// The query's sort when it has a sort clause (BRD-070).
    pub sort: Option<Vec<SortSpec>>,
    pub notes: Vec<Problem>,
    pub bindings: Vec<(Span, Binding)>,
    pub query: Query,
}

/// What `board:` does with a board in query mode (QRY-049).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryBoards {
    /// `board: Q` means «matches Q's query».
    Expand,
    /// Query boards are unknown names: inside a board query, so no query nests another.
    Leave,
}

/// What a query board's query matches for the viewer, within its projects: `False` when it lost
/// a name or does not resolve (the board shows no cards then either).
async fn query_board(
    conn: &mut PgConnection,
    viewer: &Access,
    id: BoardId,
    now: Option<OffsetDateTime>,
) -> Result<Filter> {
    let Some(board) = taskmaster_store::boards::get(&mut *conn, id).await? else {
        return Ok(Filter::False);
    };
    let Some(stored) = &board.query else {
        return Ok(Filter::False);
    };
    let (text, lost) = crate::saved_searches::text_of(&mut *conn, viewer, stored).await?;
    if !lost.is_empty() {
        return Ok(Filter::False);
    }
    match resolve_within(
        conn,
        viewer,
        &text,
        &board.projects,
        now,
        QueryBoards::Leave,
    )
    .await
    {
        Ok(w) => Ok(Filter::and(vec![
            Filter::Projects(board.projects),
            w.filter,
        ])),
        Err(Error::InvalidQuery(_)) => Ok(Filter::False),
        Err(other) => Err(other),
    }
}

/// Resolves the query by the viewer's names and clock within the projects.
pub(crate) async fn resolve_within(
    conn: &mut PgConnection,
    viewer: &Access,
    text: &str,
    projects: &[ProjectId],
    now: Option<OffsetDateTime>,
    query_boards: QueryBoards,
) -> Result<Within> {
    let q = taskmaster_query::parse(text);
    if q.problems.iter().any(|p| p.severity == Severity::Error) {
        return Err(Error::InvalidQuery(q.problems));
    }
    let input = SearchInput {
        text: text.to_owned(),
        now,
        ..SearchInput::default()
    };
    let p = resolve_parsed(
        conn,
        viewer,
        &input,
        &q,
        false,
        Some(projects),
        query_boards,
    )
    .await?;
    Ok(Within {
        filter: p.filter,
        sort: q.sort.is_some().then_some(p.sort),
        notes: p.notes,
        bindings: p.bindings,
        query: q,
    })
}
pub(crate) async fn resolve_parsed(
    conn: &mut PgConnection,
    viewer: &Access,
    input: &SearchInput,
    q: &Query,
    hide_resolved: bool,
    within: Option<&[ProjectId]>,
    query_boards: QueryBoards,
) -> Result<Prepared> {
    let clock = clock(conn, input).await?;
    let rows = catalog::projects(&mut *conn).await?;
    let mut vis = visibility(viewer, &rows);
    let projects: Vec<_> = rows
        .into_iter()
        .filter(|r| !r.deleting && sees(&vis, r.entry.id))
        .map(|r| r.entry)
        .collect();

    let mut context = Vec::new();
    if let Some(project) = input.project {
        if !projects.iter().any(|p| p.id == project) {
            return Err(Error::NotFound("no such project"));
        }
        vis.within = Some(project);
    }
    if let Some(tag) = input.tag {
        // A tag the viewer sees (ISS-037).
        let found = tags::get(&mut *conn, tag).await?;
        if found.is_none_or(|t| !t.sharing().can_view(viewer)) {
            return Err(Error::NotFound("no such tag"));
        }
        context.push(Filter::Tags(vec![tag]));
    }
    let chosen = within.is_some();
    let within: Option<Vec<ProjectId>> = within
        .map(<[ProjectId]>::to_vec)
        .or_else(|| input.project.map(|p| vec![p]));
    let live_scope = domain::scope(q, &projects, within.as_deref());
    let scope = live_scope.projects.clone();
    // Archived projects are left out unless named (PRJ-061).
    let archived: Vec<ProjectId> = projects
        .iter()
        .filter(|p| p.archived && !scope.contains(&p.id))
        .map(|p| p.id)
        .collect();
    if input.project.is_none() && !chosen && !archived.is_empty() {
        context.push(Filter::not(Filter::Projects(archived)));
    }

    let needs = domain::needs(q);
    let words: Vec<String> = needs.words.iter().cloned().collect();
    let people: Vec<String> = needs.people.iter().cloned().collect();
    let (fields, attached) = catalog::fields(&mut *conn, &scope).await?;
    let mut cat = Catalog {
        projects,
        fields,
        attached,
        ..Catalog::default()
    };
    let sets: BTreeSet<i64> = cat.attached.iter().filter_map(|a| a.set).collect();
    let whole: BTreeSet<i64> = needs
        .whole_sets
        .iter()
        .flat_map(|name| cat.fields_named(name, &scope))
        .flat_map(|f| cat.attachments(f.id, &scope))
        .filter_map(|a| a.set)
        .collect();
    let sets: Vec<i64> = sets.into_iter().collect();
    let whole: Vec<i64> = whole.into_iter().collect();
    cat.values = catalog::values(&mut *conn, &sets, &words, &whole).await?;
    cat.users = catalog::users(&mut *conn, &words, &people).await?;
    cat.groups = catalog::groups(&mut *conn, &words).await?;
    cat.tags = catalog::tags(&mut *conn, viewer.user(), viewer.groups(), &words).await?;
    cat.link_sides = catalog::link_sides(&mut *conn).await?;
    if needs.all_boards || !needs.boards.is_empty() {
        let read = viewer.projects_with(Permission::IssueRead);
        let reader = taskmaster_store::boards::Viewer {
            me: viewer.user(),
            groups: viewer.groups(),
            read: &read,
        };
        let names: Vec<String> = needs.boards.iter().cloned().collect();
        cat.boards = catalog::boards(&mut *conn, &reader, &names, needs.all_boards).await?;
        match query_boards {
            QueryBoards::Leave => cat.boards.retain(|b| b.query.is_none()),
            QueryBoards::Expand => {
                for name in &needs.boards {
                    for id in cat.boards_named(name) {
                        let Some(i) = cat
                            .boards
                            .iter()
                            .position(|b| b.id == id && b.query.is_some())
                        else {
                            continue;
                        };
                        let matches =
                            Box::pin(query_board(&mut *conn, viewer, id, input.now)).await?;
                        cat.boards[i].query = Some(matches);
                    }
                }
            }
        }
        if needs.all_boards {
            let ids: Vec<_> = cat
                .boards
                .iter()
                .filter(|b| b.query.is_none())
                .map(|b| b.id)
                .collect();
            cat.sprints = catalog::sprints(&mut *conn, &ids).await?;
        }
    }
    let ids: Vec<String> = needs.issue_ids.iter().cloned().collect();
    cat.issues = catalog::issues(&mut *conn, &ids, &vis).await?;
    if !needs.texts.is_empty() {
        let texts: Vec<String> = needs.texts.iter().cloned().collect();
        cat.common_words = catalog::common_words(&mut *conn, &texts)
            .await?
            .into_iter()
            .collect();
    }

    let ctx = Ctx {
        me: viewer.user(),
        clock,
        private_readable: union(
            viewer.projects_with(Permission::IssuePrivateRead),
            viewer.projects_with(Permission::VisibilityBypass),
        ),
        scope,
        hide_resolved,
    };
    let resolved = domain::resolve(q, &cat, &ctx).map_err(Error::InvalidQuery)?;
    context.insert(0, resolved.filter);
    Ok(Prepared {
        filter: Filter::and(context),
        sort: resolved.sort,
        vis,
        notes: resolved.notes,
        canonical: taskmaster_query::print(q),
        single_id: resolved.single_id,
        concerns_resolution: resolved.concerns_resolution,
        bindings: resolved.bindings,
        scope: live_scope,
    })
}

/// A query limited to one project as a whole (`project: WEB …`) is a list within it: the
/// visibility predicate for that project alone, and no second condition on the project, whose
/// selectivity the planner would multiply with the first (D-035).
fn within_one(filter: Filter, vis: &mut Visibility) -> Filter {
    let mut parts = match filter {
        Filter::And(parts) => parts,
        other => vec![other],
    };
    let one = parts
        .iter()
        .position(|part| matches!(part, Filter::Projects(ids) if ids.len() == 1));
    let Some(index) = one else {
        return Filter::and(parts);
    };
    if let Filter::Projects(ids) = parts.remove(index) {
        vis.within = ids.first().copied();
    }
    Filter::and(parts)
}

/// The request's zone, else the instance's.
pub(crate) async fn clock(conn: &mut PgConnection, input: &SearchInput) -> Result<Clock> {
    let name = match &input.time_zone {
        Some(name) => name.clone(),
        None => settings::value(&mut *conn, "default_time_zone")
            .await?
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "UTC".to_owned()),
    };
    let zone = jiff::tz::TimeZone::get(&name)
        .map_err(|_| Error::field("time_zone", InputProblem::Invalid, "unknown time zone"))?;
    let now = input.now.unwrap_or_else(OffsetDateTime::now_utc);
    let now = jiff::Timestamp::new(
        now.unix_timestamp(),
        i32::try_from(now.nanosecond()).unwrap_or(0),
    )
    .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Clock { now, zone })
}

fn union(a: Projects, b: Projects) -> Projects {
    match (a, b) {
        (Projects::All, _) | (_, Projects::All) => Projects::All,
        (Projects::Only(mut a), Projects::Only(b)) => {
            a.extend(b);
            a.sort_unstable();
            a.dedup();
            Projects::Only(a)
        }
    }
}

/// The viewer's access as the store's predicate needs it; projects being deleted are out.
pub(crate) fn visibility(viewer: &Access, rows: &[ProjectRow]) -> Visibility {
    let all: Vec<ProjectId> = rows.iter().map(|r| r.entry.id).collect();
    let deleting: Vec<ProjectId> = rows
        .iter()
        .filter(|r| r.deleting)
        .map(|r| r.entry.id)
        .collect();
    visibility_over(viewer, &all, &deleting)
}

/// The viewer's access over these projects (`all`), those being deleted left out.
pub(crate) fn visibility_over(
    viewer: &Access,
    all: &[ProjectId],
    deleting: &[ProjectId],
) -> Visibility {
    let live = |projects: Projects| -> Projects {
        let ids: Vec<ProjectId> = match projects {
            Projects::All if deleting.is_empty() => return Projects::All,
            Projects::All => all.to_vec(),
            Projects::Only(ids) => ids,
        };
        Projects::Only(ids.into_iter().filter(|p| !deleting.contains(p)).collect())
    };
    let read = live(viewer.projects_with(Permission::IssueRead));
    let create = live(viewer.projects_with(Permission::IssueCreate));
    let create_only = match create {
        Projects::All => all.to_vec(),
        Projects::Only(ids) => ids,
    }
    .into_iter()
    .filter(|p| !deleting.contains(p) && !holds(&read, *p))
    .collect();
    Visibility {
        me: viewer.user(),
        read,
        create_only,
        bypass: live(viewer.projects_with(Permission::VisibilityBypass)),
        comment_read: live(viewer.projects_with(Permission::CommentRead)),
        groups: viewer.groups().to_vec(),
        within: None,
    }
}

/// The viewer sees some issues of the project.
pub(crate) fn sees(vis: &Visibility, project: ProjectId) -> bool {
    holds(&vis.read, project) || vis.create_only.contains(&project)
}

/// What a cursor is bound to: a cursor of another query is refused (D-016).
fn fingerprint(canonical: &str, input: &SearchInput) -> String {
    let text = format!(
        "{canonical}\u{0}{:?}\u{0}{:?}\u{0}{:?}\u{0}{}\u{0}{:?}{}",
        input.project,
        input.tag,
        input.saved_search,
        input.hide_resolved,
        input.time_zone,
        input
            .manual_order
            .map_or(String::new(), |m| format!("\u{0}manual {m}"))
    );
    // FNV-1a: stable across builds, unlike the standard hasher.
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// Where a page ended: in the ranked phase of a manual order, or in the sorted one.
#[derive(Debug, Clone, PartialEq)]
enum At {
    Ranked(String, IssueId),
    Sorted(Vec<SortValue>),
}

#[derive(Serialize, Deserialize)]
struct Cursor {
    h: String,
    k: Vec<SortValue>,
    /// The phase one position of a manual order: its rank and issue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    r: Option<(String, IssueId)>,
}

fn write_cursor(hash: &str, at: &At) -> String {
    let (k, r) = match at {
        At::Ranked(rank, id) => (Vec::new(), Some((rank.clone(), *id))),
        At::Sorted(keys) => (keys.clone(), None),
    };
    let body = serde_json::to_vec(&Cursor {
        h: hash.to_owned(),
        k,
        r,
    })
    .unwrap_or_default();
    URL_SAFE_NO_PAD.encode(body)
}

fn read_cursor(text: &str, hash: &str, keys: usize, manual: bool) -> Result<At> {
    let invalid = || {
        Error::field(
            "cursor",
            InputProblem::Invalid,
            "the cursor is not of this query",
        )
    };
    let bytes = URL_SAFE_NO_PAD.decode(text).map_err(|_| invalid())?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if cursor.h != hash {
        return Err(invalid());
    }
    match cursor.r {
        Some((rank, id)) if manual && cursor.k.is_empty() => Ok(At::Ranked(rank, id)),
        None if cursor.k.len() == keys => Ok(At::Sorted(cursor.k)),
        _ => Err(invalid()),
    }
}
