//! Load scenarios: one per line of the performance budget (architecture.md §7) that the schema
//! can serve so far. An operation is one request. Scenarios the server answers already are whole
//! requests over HTTP (`[http]`, D-035); the others (`[sql]`) would run what the server would
//! query, measured from the first query to the last decoded row, until their endpoints exist.
//! Boards are over HTTP since step 1.8a; no scenario is `[sql]` now.

mod board;
mod command;
mod export;
mod issue;
mod list;
pub mod live;
mod search;

use std::borrow::Cow;
use std::time::Duration;

use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::Query as SqlxQuery;
use sqlx::{AssertSqlSafe, PgConnection, Postgres, Row};

use crate::catalog::Catalog;
use crate::http::Node;
use crate::rng::Rng;

/// How an operation is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A whole request to the node, from authentication to the decoded answer.
    Http,
    /// The queries the server would make, on one connection.
    Sql,
}

impl Mode {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Http => "[http]",
            Self::Sql => "[sql]",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    List,
    Board,
    Search,
    Open,
    Change,
    Command,
    Export,
}

impl Scenario {
    pub const ALL: [Self; 7] = [
        Self::List,
        Self::Board,
        Self::Search,
        Self::Open,
        Self::Change,
        Self::Command,
        Self::Export,
    ];

    /// Name on the command line.
    pub fn key(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Board => "board",
            Self::Search => "search",
            Self::Open => "issue",
            Self::Change => "change",
            Self::Command => "command",
            Self::Export => "export",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::List => "Issue list page of 50",
            Self::Board => "Board with 500 cards",
            Self::Search => "Full-text query",
            Self::Open => "Open issue",
            Self::Change => "Field change commit",
            Self::Command => "Command apply",
            Self::Export => "Issue export of 500",
        }
    }

    /// Server time at p95 from the budget.
    pub fn budget(self) -> Duration {
        Duration::from_millis(match self {
            Self::List => 50,
            Self::Board => 80,
            Self::Search => 150,
            Self::Export => 300,
            Self::Open | Self::Change | Self::Command => 30,
        })
    }

    /// Why the scenario may stay over budget for now, when what it measures is being replaced;
    /// none since step 1.5a (D-035) but three: the change scenario's rule variants (owner,
    /// 2026-11-06, still, D-049) — the walk's writes are batched since 1.9b-7 and the variants
    /// came down with it, yet a few milliseconds over the 30 stay; since the real-server run
    /// of 1.12a the list scenario's «fixed bugs in all projects» tail for viewers whose early
    /// positions do not fill a page (owner, 2026-11-11, D-054: the union by position lifted
    /// every other variant and the medians; the deep branches are D-035's «three large sets»,
    /// whose answers — the covering visibility index, Tantivy — move to P2); and since 2.1a-3
    /// the command scenario's two — the bulk variant's p95 is a whole job's (step 2.1 §8), and
    /// the one-issue apply sits a few ms over
    /// the change budget (p95 34.9 of 30 ms on the quiet server; ~3× on the loaded dev Mac).
    pub fn known_miss(self) -> Option<&'static str> {
        match self {
            Self::List => Some("the «fixed bugs in all projects» tail, D-054"),
            Self::Board | Self::Search | Self::Open | Self::Export => None,
            Self::Change => Some("the rule variants (state cascade, card move), D-049"),
            Self::Command => Some(
                "the bulk variant is one whole job over ≈50 issues — its p95 is the job's, \
                 not one apply's (step 2.1 §8) — and the one-issue variant sits a few ms \
                 over the change budget even on the quiet server (p95 34.9 of 30 ms, \
                 2.1a-3: several writes and a comment, not one field change) — the D-049 \
                 pattern; the loaded dev Mac showed ~3×",
            ),
        }
    }

    /// Part of the benchmark gate of D-022.
    pub fn gate(self) -> bool {
        matches!(self, Self::List | Self::Board | Self::Search)
    }

    /// Measured operations of this scenario, from `--iterations`. The command scenario caps at
    /// 40 (step 2.1 §8): one bulk operation is a whole job over ≈50 issues and takes seconds, so
    /// the default 200 would hold the bench for many minutes for samples it does not need.
    pub fn iterations(self, run: &crate::runner::RunOptions) -> usize {
        match self {
            Self::Command => run.iterations.min(40),
            _ => run.iterations,
        }
    }

    /// Wall time this scenario may take beyond the bench's base budget (`report::Timing`):
    /// the command scenario's 40 operations plus their warm-up pass are expected to run for
    /// minutes — whole jobs, not milliseconds — and the base budget (D-032) watches the rest.
    pub fn time_allowance(self) -> Duration {
        match self {
            Self::Command => Duration::from_secs(4 * 60),
            _ => Duration::ZERO,
        }
    }

    /// Names of the query variants, used in turn.
    pub fn variants(self) -> &'static [&'static str] {
        match self {
            Self::List => list::VARIANTS,
            Self::Board => board::VARIANTS,
            Self::Search => search::VARIANTS,
            Self::Open => &["issue, activity, files, links"],
            Self::Change => &[
                "priority change",
                "state cascade",
                "assignee warning",
                "card move",
            ],
            Self::Command => &["one issue", "bulk job"],
            Self::Export => &["csv", "xlsx"],
        }
    }

    /// How variant `variant` is measured.
    pub fn mode(self, _variant: usize) -> Mode {
        match self {
            Self::List
            | Self::Board
            | Self::Search
            | Self::Open
            | Self::Change
            | Self::Command
            | Self::Export => Mode::Http,
        }
    }

    /// The tag of the scenario's lines: one mode, or both.
    pub fn tag(self) -> &'static str {
        let modes: Vec<Mode> = (0..self.variants().len()).map(|v| self.mode(v)).collect();
        if modes.iter().all(|m| *m == Mode::Http) {
            Mode::Http.tag()
        } else if modes.iter().all(|m| *m == Mode::Sql) {
            Mode::Sql.tag()
        } else {
            "[sql+http]"
        }
    }

    /// Runs one operation of variant `variant`; returns the rows of its main result.
    pub async fn run(
        self,
        catalog: &Catalog,
        node: &Node,
        conn: &mut PgConnection,
        rng: &mut Rng,
        variant: usize,
    ) -> anyhow::Result<usize> {
        match self {
            Self::List => list::run(catalog, node, rng, variant).await,
            Self::Board => board::view(catalog, node, conn, rng, variant).await,
            Self::Search => search::run(catalog, node, rng, variant).await,
            Self::Open => issue::open(catalog, node, rng).await,
            Self::Change if variant == 1 => issue::state_cascade(catalog, node, conn, rng).await,
            Self::Change if variant == 2 => issue::assignee_warning(catalog, node, conn, rng).await,
            Self::Change if variant == 3 => board::move_card(catalog, node, conn, rng).await,
            Self::Command if variant == 1 => command::bulk(catalog, node, conn, rng).await,
            Self::Command => command::one(catalog, node, conn, rng).await,
            Self::Export => export::run(catalog, node, rng, variant).await,
            Self::Change => issue::change(catalog, node, rng).await,
        }
    }

    /// The main query of variant `variant`, for `EXPLAIN`; none for requests over HTTP
    /// (`--breakdown` shows their statements).
    pub fn main_query(self, catalog: &Catalog, rng: &mut Rng, variant: usize) -> Option<Query> {
        if self.mode(variant) == Mode::Http {
            return None;
        }
        let _ = (catalog, rng);
        None
    }
}

/// A query with its parameters, so the same query can be timed and explained.
#[derive(Debug, Clone)]
pub struct Query {
    /// Built by the benchmark's own code; values are always parameters.
    pub sql: Cow<'static, str>,
    pub binds: Vec<Bind>,
}

#[derive(Debug, Clone)]
pub enum Bind {
    Int(i32),
    BigInt(i64),
    BigInts(Vec<i64>),
    Text(String),
    Texts(Vec<String>),
    Time(time::OffsetDateTime),
}

impl Query {
    pub fn new(sql: &'static str, binds: Vec<Bind>) -> Self {
        Self {
            sql: Cow::Borrowed(sql),
            binds,
        }
    }

    pub fn built(sql: String, binds: Vec<Bind>) -> Self {
        Self {
            sql: Cow::Owned(sql),
            binds,
        }
    }

    fn bind<'q>(
        &self,
        mut query: SqlxQuery<'q, Postgres, PgArguments>,
    ) -> SqlxQuery<'q, Postgres, PgArguments> {
        for bind in &self.binds {
            query = match bind {
                Bind::Int(value) => query.bind(*value),
                Bind::BigInt(value) => query.bind(*value),
                Bind::BigInts(values) => query.bind(values.clone()),
                Bind::Text(value) => query.bind(value.clone()),
                Bind::Texts(values) => query.bind(values.clone()),
                Bind::Time(value) => query.bind(*value),
            };
        }
        query
    }

    pub async fn fetch(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<PgRow>> {
        self.run(conn, self.sql.to_string()).await
    }

    /// Runs as an unnamed statement, so PostgreSQL plans it for these parameter values: a cached
    /// generic plan does not see them and goes wrong on skewed data (D-024).
    async fn run(&self, conn: &mut PgConnection, sql: String) -> sqlx::Result<Vec<PgRow>> {
        self.bind(sqlx::query(AssertSqlSafe(sql)))
            .persistent(false)
            .fetch_all(conn)
            .await
    }

    /// IDs from the first column.
    pub async fn ids(&self, conn: &mut PgConnection) -> sqlx::Result<Vec<i64>> {
        self.fetch(conn)
            .await?
            .iter()
            .map(|row| row.try_get::<i64, _>(0))
            .collect()
    }

    /// The executed plan with timings and buffer use.
    pub async fn explain(&self, conn: &mut PgConnection) -> sqlx::Result<String> {
        let sql = format!("explain (analyze, buffers, verbose off) {}", self.sql);
        let rows = self.run(conn, sql).await?;
        rows.iter()
            .map(|row| row.try_get::<String, _>(0))
            .collect::<sqlx::Result<Vec<_>>>()
            .map(|lines| lines.join("\n"))
    }
}

/// Binds parameters to a fixed query of the server's (`crates/store`): a prepared statement,
/// as the compile-checked macros make.
fn fixed<'q>(sql: &'static str, binds: &[Bind]) -> SqlxQuery<'q, Postgres, PgArguments> {
    Query::new(sql, binds.to_vec()).bind(sqlx::query(sql))
}

/// Runs a fixed query; the rows are read whole but not converted (the server converts them into
/// structs, which costs little next to the database).
async fn rows(
    conn: &mut PgConnection,
    sql: &'static str,
    binds: &[Bind],
) -> sqlx::Result<Vec<PgRow>> {
    let rows = fixed(sql, binds).fetch_all(conn).await?;
    for row in &rows {
        for column in 0..row.len() {
            let _ = row.try_get_raw(column)?.as_bytes();
        }
    }
    Ok(rows)
}

fn ids_in(rows: &[PgRow], column: usize) -> sqlx::Result<Vec<i64>> {
    rows.iter()
        .map(|row| row.try_get::<i64, _>(column))
        .collect()
}

const ISSUES: &str = "
select id, project_id, number, summary, description, reporter_id, created_at, updated_at,
       updated_by, resolved_at, deleted_at, deleted_by, audience_users, audience_groups,
       votes, version, text_version, description_html, mentioned_users, issue_tokens,
       file_names
from issues where id = any($1)";
const PROJECTS: &str = "
select id, key, name, description, description_html, kind, template, owner_id, logo_color, next_number,
       default_audience_group_id, archived_at, deleting_at, deletion_job_id, created_by, version,
       created_at, updated_at
from projects where id = any($1)";
const PROJECT_FIELDS: &str = "
select project_id, field_id, set_id, rank, can_be_empty, empty_label, default_values,
       private, version, created_at
from project_fields where project_id = any($1) order by project_id, rank, field_id";
const FIELDS: &str = "
select id, name, kind, multi, aliases, builtin, auto_attach, rank, set_id, can_be_empty,
       empty_label, private, default_values, version, created_at
from fields where id = any($1)";
const REF_VALUES: &str = "
select issue_id, field_id, value from issue_ref_values where issue_id = any($1)
order by issue_id, field_id, value";
/// The other value tables, read the same way.
const SCALAR_VALUES: [&str; 6] = [
    "select issue_id, field_id, value from issue_int_values where issue_id = any($1)",
    "select issue_id, field_id, value from issue_float_values where issue_id = any($1)",
    "select issue_id, field_id, value from issue_date_values where issue_id = any($1)",
    "select issue_id, field_id, value from issue_datetime_values where issue_id = any($1)",
    "select issue_id, field_id, value from issue_string_values where issue_id = any($1)",
    "select issue_id, field_id, value from issue_text_values where issue_id = any($1)",
];
/// Renderings of text values (`store::content::text_values_of`).
const TEXT_HTML: &str = "
select issue_id as issue, field_id as field, html, mentioned_users as users, issue_tokens,
       file_names as files
from issue_text_values where issue_id = any($1)";
const VIEWER_TAGS: &str = "
select it.issue_id, t.id, t.name, t.owner_id, t.color, t.version, t.created_at
from issue_tags it join tags t on t.id = it.tag_id
where it.issue_id = any($1) and t.owner_id = $2
order by it.issue_id, lower(t.name), t.id";
const WATCHED: &str =
    "select issue_id from issue_watchers where issue_id = any($1) and user_id = $2";
const VOTED: &str = "select issue_id from issue_votes where issue_id = any($1) and user_id = $2";
const WATCHER_COUNTS: &str = "
select issue_id, count(*) from issue_watchers where issue_id = any($1) group by issue_id";
const USERS: &str = "
select id, username, full_name, email, email_verified_at, builtin, avatar_color,
       disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version, created_at
from users where id = any($1)";
const GROUPS: &str = "
select id, name, description, builtin, auto_join, team_of, version, created_at
from groups where id = any($1)";
const VALUES: &str = "
select id, set_id, name, description, color, rank, archived, resolved, released,
       release_date, start_date, assembled_on, owner_id, version, created_at
from field_values where id = any($1)";

/// The fields of projects with their definitions (`service::issues::values::of_projects`):
/// `(project, field)` pairs and the IDs of user and group fields.
async fn project_fields(
    conn: &mut PgConnection,
    projects: &[i64],
) -> sqlx::Result<(Vec<(i64, i64)>, Vec<i64>, Vec<i64>)> {
    let attached = rows(conn, PROJECT_FIELDS, &[Bind::BigInts(projects.to_vec())]).await?;
    let pairs: Vec<(i64, i64)> = attached
        .iter()
        .map(|row| Ok((row.try_get(0)?, row.try_get(1)?)))
        .collect::<sqlx::Result<_>>()?;
    let mut ids: Vec<i64> = pairs.iter().map(|p| p.1).collect();
    ids.sort_unstable();
    ids.dedup();
    let defs = rows(conn, FIELDS, &[Bind::BigInts(ids)]).await?;
    let of_kind = |kind: &str| -> sqlx::Result<Vec<i64>> {
        let mut out = Vec::new();
        for row in &defs {
            if row.try_get::<&str, _>(2)? == kind {
                out.push(row.try_get(0)?);
            }
        }
        Ok(out)
    };
    Ok((pairs, of_kind("user")?, of_kind("group")?))
}

/// Field values of issues: one query per value table (`store::issue_values::of_issues`); the
/// reference rows as `(issue, field, value)`.
async fn values(conn: &mut PgConnection, ids: &[i64]) -> sqlx::Result<Vec<(i64, i64, i64)>> {
    let refs = rows(conn, REF_VALUES, &[Bind::BigInts(ids.to_vec())]).await?;
    for sql in SCALAR_VALUES {
        rows(conn, sql, &[Bind::BigInts(ids.to_vec())]).await?;
    }
    refs.iter()
        .map(|row| Ok((row.try_get(0)?, row.try_get(1)?, row.try_get(2)?)))
        .collect()
}

/// Subtasks of a page of issues (`store::links::children_of`, LNK-014).
pub const CHILDREN: &str = "
select l.source_id as parent, o.id, o.project_id, o.number, o.summary, o.reporter_id,
       o.audience_users, o.audience_groups, o.resolved_at, o.deleted_at
from issue_links l
join link_types t on t.id = l.type_id and t.builtin = 'subtask'
join issues o on o.id = l.target_id
where l.source_id = any($1)";

/// What answers show of a page of issues, as `service::issues::describe` loads it: the issues,
/// their projects and fields, every value, the viewer's tags, stars and votes, watcher counts, and
/// the people, groups and values they name. One query per relation for the whole page
/// (architecture.md §4). Answers the issues found.
pub async fn describe(conn: &mut PgConnection, viewer: i64, ids: &[i64]) -> sqlx::Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let issues = rows(conn, ISSUES, &[Bind::BigInts(ids.to_vec())]).await?;
    let issues = &issues;
    let ids = ids_in(issues, 0)?;
    let mut projects = ids_in(issues, 1)?;
    projects.sort_unstable();
    projects.dedup();
    rows(conn, PROJECTS, &[Bind::BigInts(projects.clone())]).await?;
    let (_, user_fields, group_fields) = project_fields(conn, &projects).await?;
    let refs = values(conn, &ids).await?;
    let page = [Bind::BigInts(ids.clone()), Bind::BigInt(viewer)];
    rows(conn, VIEWER_TAGS, &page).await?;
    rows(conn, WATCHED, &page).await?;
    rows(conn, VOTED, &page).await?;
    rows(conn, WATCHER_COUNTS, &page[..1]).await?;
    rows(conn, TEXT_HTML, &page[..1]).await?;
    // Subtask counts (LNK-014), with the projects of subtasks elsewhere.
    let children = rows(conn, CHILDREN, &page[..1]).await?;
    let mut elsewhere: Vec<i64> = ids_in(&children, 2)?
        .into_iter()
        .filter(|p| !projects.contains(p))
        .collect();
    elsewhere.sort_unstable();
    elsewhere.dedup();
    if !elsewhere.is_empty() {
        rows(conn, PROJECTS, &[Bind::BigInts(elsewhere)]).await?;
    }

    let (mut people, mut groups, mut values) = (Vec::new(), Vec::new(), Vec::new());
    for issue in issues {
        people.push(issue.try_get::<i64, _>(5)?);
        people.extend(issue.try_get::<Option<i64>, _>(8)?);
        people.extend(issue.try_get::<Vec<i64>, _>(12)?);
        groups.extend(issue.try_get::<Vec<i64>, _>(13)?);
    }
    for &(_, field, value) in &refs {
        if user_fields.contains(&field) {
            people.push(value);
        } else if group_fields.contains(&field) {
            groups.push(value);
        } else {
            values.push(value);
        }
    }
    for list in [&mut people, &mut groups, &mut values] {
        list.sort_unstable();
        list.dedup();
    }
    rows(conn, USERS, &[Bind::BigInts(people)]).await?;
    rows(conn, GROUPS, &[Bind::BigInts(groups)]).await?;
    rows(conn, VALUES, &[Bind::BigInts(values)]).await?;
    Ok(issues.len())
}
