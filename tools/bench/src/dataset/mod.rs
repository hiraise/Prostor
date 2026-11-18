//! The benchmark's dataset (architecture.md §7), generated into a migrated database: the real tables
//! where they exist (users with API tokens, groups, roles and their assignments, projects, fields,
//! value sets, issues with their values, history, watchers, votes and tags, sprint boards with
//! their cards). It grows with the schema: a step that adds tables teaches the generator to fill
//! them. The prototype tables of D-022 are gone since step 1.8a.
//!
//! The issue tables are loaded bare, as the prototype was: their keys, foreign keys and indexes are
//! read from the catalog, dropped, and built again after the load from those definitions, several
//! at a time. The generator then checks that the schema is exactly what the migrations made. A
//! load that fails halfway leaves no fingerprint, so the next run starts from a new database.

pub mod copy;
pub mod issues;
pub mod model;

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Context;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, Connection, PgConnection};
use taskmaster_domain::access::permission::PermissionSet;
use taskmaster_domain::auth::Scopes;
use taskmaster_domain::projects::Template;
use taskmaster_domain::rank;
use taskmaster_domain::rules::{self, Rule};
use taskmaster_service::secrets::TOKEN_PREFIX;
use taskmaster_store::PgConnectOptions;

use self::copy::{CopyBuf, execute, execute_built};
use self::issues::END;
use self::model::{Kind, Model};
use crate::rng::Rng;
use crate::text::Vocabulary;

/// Raise when the generator writes something different: datasets of other versions are rebuilt.
pub const GENERATOR_VERSION: u32 = 9;
/// The default seed; another seed gives another dataset of the same shape.
pub const SEED: u64 = 1;

/// Bookkeeping of the benchmark database.
const BENCH_SQL: &str = "
create schema bench;
create table bench.dataset (
    fingerprint text not null,
    scale double precision not null,
    seed bigint not null,
    seconds double precision not null,
    -- Rows per table, counted after the load.
    tables jsonb not null,
    generated_at timestamptz not null default now()
);";

/// Tables the dataset fills.
const REAL_TABLES: [&str; 36] = [
    "users",
    "api_tokens",
    "groups",
    "group_members",
    "roles",
    "role_assignments",
    "projects",
    "project_keys",
    "value_sets",
    "field_values",
    "value_set_groups",
    "fields",
    "project_fields",
    "project_rules",
    "tags",
    "issues",
    "issue_ref_values",
    "issue_int_values",
    "issue_float_values",
    "issue_date_values",
    "issue_datetime_values",
    "issue_string_values",
    "issue_text_values",
    "issue_history",
    "comments",
    "attachments",
    "link_types",
    "issue_links",
    "issue_watchers",
    "issue_votes",
    "issue_tags",
    "boards",
    "board_projects",
    "board_columns",
    "sprints",
    "sprint_issues",
];

/// Rows loaded with their IDs leave the identity sequences behind; the server's inserts continue
/// after them.
const SEQUENCES_SQL: &str = "
select setval(pg_get_serial_sequence('groups', 'id'), (select max(id) from groups));
select setval(pg_get_serial_sequence('projects', 'id'), (select max(id) from projects));
select setval(pg_get_serial_sequence('value_sets', 'id'), (select max(id) from value_sets));
select setval(pg_get_serial_sequence('field_values', 'id'), (select max(id) from field_values));
select setval(pg_get_serial_sequence('fields', 'id'), (select max(id) from fields));
select setval(pg_get_serial_sequence('tags', 'id'), (select max(id) from tags));
select setval(pg_get_serial_sequence('boards', 'id'), (select coalesce(max(id), 0) + 1 from boards), false);
select setval(pg_get_serial_sequence('sprints', 'id'), (select coalesce(max(id), 0) + 1 from sprints), false);";

/// The built-in link types (LNK-002) with the IDs the generator uses (`issues::RELATED`…).
const LINK_TYPES_SQL: &str = "
insert into link_types (id, name, source_name, target_name, direction, builtin, locked)
overriding system value values
    (1, 'Related', 'related to', null, 'undirected', 'related', false),
    (2, 'Dependency', 'blocks', 'blocked by', 'directed', 'dependency', false),
    (3, 'Duplicate', 'has duplicate', 'duplicate of', 'hierarchy', 'duplicate', true),
    (4, 'Subtask', 'parent of', 'child of', 'hierarchy', 'subtask', true);
select setval(pg_get_serial_sequence('link_types', 'id'), 4);";

/// Access as the server grants it (`service::projects::create`): owners administer their projects,
/// project teams are members there, and people who reported issues in a project whose team they
/// are not in hold a custom role that lets them create issues and comment there. Nobody sees every
/// audience: the predefined roles do not include it.
const ACCESS_SQL: &str = "
insert into role_assignments (role_id, user_id, project_id)
select r.id, p.owner_id, p.id from projects p, roles r where r.builtin = 'project_admin';
insert into role_assignments (role_id, group_id, project_id)
select r.id, g.id, g.team_of from groups g, roles r
where r.builtin = 'project_member' and g.team_of is not null;
insert into role_assignments (role_id, user_id, project_id)
select distinct r.id, i.reporter_id, i.project_id
from issues i, roles r
where r.builtin is null and r.name = 'Reporter'
  and not exists (select from group_members m join groups g on g.id = m.group_id
                  where g.team_of = i.project_id and m.user_id = i.reporter_id)
  and not exists (select from projects p where p.id = i.project_id and p.owner_id = i.reporter_id);";

/// Issue tables loaded in bulk: they lose their keys and indexes during the load.
const BULK_TABLES: [&str; 16] = [
    "issues",
    "issue_ref_values",
    "issue_int_values",
    "issue_float_values",
    "issue_date_values",
    "issue_datetime_values",
    "issue_string_values",
    "issue_text_values",
    "issue_history",
    "comments",
    "attachments",
    "issue_links",
    "issue_watchers",
    "issue_votes",
    "issue_tags",
    "sprint_issues",
];

/// Sequences of the bulk tables, after rows loaded with their IDs.
const ISSUE_SEQUENCES_SQL: &str = "
select setval(pg_get_serial_sequence('issues', 'id'), (select max(id) from issues));
select setval(pg_get_serial_sequence('issue_history', 'id'), (select max(id) from issue_history));
select setval(pg_get_serial_sequence('comments', 'id'), (select max(id) from comments));
select setval(pg_get_serial_sequence('attachments', 'id'), (select max(id) from attachments));
select setval(pg_get_serial_sequence('issue_links', 'id'), (select max(id) from issue_links));";

/// Issues per unit of work: generated in memory, then sent with COPY.
const CHUNK: u64 = 25_000;
/// Parallel connections while loading.
const WORKERS: usize = 4;

/// The target dataset of the budget: 100k issues, a team of up to 50 people (D-032). Scale 1 (1M
/// issues) is the stress dataset; tests use a small fraction.
pub const TARGET_SCALE: f64 = 0.1;

/// What to generate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub scale: f64,
    pub seed: u64,
}

impl Plan {
    pub fn new(scale: f64, seed: u64) -> Self {
        Self { scale, seed }
    }

    pub fn issues(&self) -> u64 {
        ((1_000_000.0 * self.scale).round() as u64).max(100)
    }

    pub fn projects(&self) -> usize {
        ((200.0 * self.scale).round() as usize).clamp(5, 200)
    }

    pub fn users(&self) -> usize {
        ((10_000.0 * self.scale).round() as usize).max(50)
    }

    /// Changes whenever the dataset this plan produces would change: the generator, the
    /// migrations, the scale or the seed.
    pub fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(GENERATOR_VERSION.to_le_bytes());
        for migration in taskmaster_store::MIGRATOR.iter() {
            hash.update(migration.version.to_le_bytes());
            hash.update(&*migration.checksum);
        }
        hash.update(self.scale.to_le_bytes());
        hash.update(self.seed.to_le_bytes());
        hash.finalize()[..8]
            .iter()
            .fold(String::new(), |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            })
    }
}

/// Rows per table after generation.
#[derive(Debug, Clone)]
pub struct Summary {
    pub tables: Vec<(String, i64)>,
    pub seconds: f64,
}

/// The fingerprint of the dataset in the database, if one was generated there.
pub async fn fingerprint(conn: &mut PgConnection) -> sqlx::Result<Option<String>> {
    let exists: bool = sqlx::query_scalar("select to_regclass('bench.dataset') is not null")
        .fetch_one(&mut *conn)
        .await?;
    if !exists {
        return Ok(None);
    }
    sqlx::query_scalar("select fingerprint from bench.dataset")
        .fetch_optional(conn)
        .await
}

/// A connection for loading: no statement timeout, no wait for the disk on commit, room to sort.
pub async fn connect(options: &PgConnectOptions) -> sqlx::Result<PgConnection> {
    PgConnection::connect_with(&options.clone().options([
        ("statement_timeout", "0"),
        ("synchronous_commit", "off"),
        ("maintenance_work_mem", "512MB"),
    ]))
    .await
}

/// Generates the dataset of `plan` into a migrated database without it; `log` hears each stage.
pub async fn generate(
    options: &PgConnectOptions,
    plan: Plan,
    log: &(dyn Fn(&str) + Sync),
) -> anyhow::Result<Summary> {
    let started = Instant::now();
    let mut conn = connect(options).await?;
    execute(&mut conn, BENCH_SQL).await?;

    log(&format!("users: {}", plan.users()));
    users_rows(plan).send(&mut conn).await?;
    let users: Vec<i64> = sqlx::query_scalar("select id from users order by id")
        .fetch_all(&mut conn)
        .await?;
    let (first_group, all_members): (i64, i64) = sqlx::query_as(
        "select (select max(id) + 1 from groups), id from groups where builtin = 'all_members'",
    )
    .fetch_one(&mut conn)
    .await?;
    let model = tokio::task::spawn_blocking(move || Model::build(plan, users, first_group)).await?;
    let model = Arc::new(model);
    for mut rows in metadata_rows(&model, all_members) {
        rows.send(&mut conn).await?;
    }
    tags_rows(&model).send(&mut conn).await?;
    execute(&mut conn, SEQUENCES_SQL).await?;
    execute(&mut conn, LINK_TYPES_SQL).await?;

    let schema = Schema::read(&mut conn).await?;
    schema.drop(&mut conn).await?;
    log(&format!(
        "issues: {} in {} projects, with field values, history, watchers, votes and tags",
        model.issue_count(),
        model.projects.len()
    ));
    load_issues(options, &model).await?;
    execute(&mut conn, ISSUE_SEQUENCES_SQL).await?;
    log("roles and API tokens");
    access(&mut conn, plan).await?;

    log("keys and indexes");
    schema.restore(options, &mut conn).await?;

    log("vacuum and statistics");
    let tables = tables();
    let vacuum = tables
        .iter()
        .map(|table| Job::Sql(format!("vacuum (analyze) {table}")))
        .collect();
    run_parallel(options, vacuum).await?;

    let mut counts = Vec::with_capacity(tables.len());
    for table in &tables {
        let sql = format!("select count(*) from {table}");
        let rows: i64 = sqlx::query_scalar(AssertSqlSafe(sql))
            .fetch_one(&mut conn)
            .await?;
        counts.push(format!(r#""{table}": {rows}"#));
    }
    let seconds = started.elapsed().as_secs_f64();
    sqlx::query(
        "insert into bench.dataset (fingerprint, scale, seed, seconds, tables)
         values ($1, $2, $3, $4, $5::jsonb)",
    )
    .bind(plan.fingerprint())
    .bind(plan.scale)
    .bind(i64::try_from(plan.seed)?)
    .bind(seconds)
    .bind(format!("{{{}}}", counts.join(", ")))
    .execute(&mut conn)
    .await?;
    let tables = row_counts(&mut conn).await?;
    conn.close().await?;
    Ok(Summary { tables, seconds })
}

/// Generates issues in chunks on several connections: one chunk is generated in memory on a
/// blocking thread, then sent table by table with COPY.
async fn load_issues(options: &PgConnectOptions, model: &Arc<Model>) -> anyhow::Result<()> {
    let total = model.issue_count();
    let chunks: VecDeque<Range<u64>> = (0..total.div_ceil(CHUNK))
        .map(|n| n * CHUNK + 1..((n + 1) * CHUNK).min(total) + 1)
        .collect();
    let chunks = Arc::new(Mutex::new(chunks));
    let mut workers = tokio::task::JoinSet::new();
    for _ in 0..WORKERS {
        let (options, model, chunks) = (options.clone(), Arc::clone(model), Arc::clone(&chunks));
        workers.spawn(async move {
            let mut conn = connect(&options).await?;
            loop {
                let next = chunks.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some(range) = next else { break };
                let model = Arc::clone(&model);
                let mut rows =
                    tokio::task::spawn_blocking(move || issues::generate(&model, range)).await?;
                for buf in rows.all() {
                    buf.send(&mut conn).await?;
                }
            }
            conn.close().await?;
            Ok::<_, anyhow::Error>(())
        });
    }
    while let Some(done) = workers.join_next().await {
        done??;
    }
    Ok(())
}

/// Keys, foreign keys and indexes of the bulk tables as the migrations made them: dropped before the
/// load and built again after it.
#[derive(Debug, Clone, PartialEq)]
struct Schema {
    /// Primary keys and unique constraints: (table, name, definition).
    keys: Vec<(String, String, String)>,
    /// Foreign keys of the bulk tables and those that refer to them.
    foreign: Vec<(String, String, String)>,
    /// Indexes that back no constraint: (name, definition).
    indexes: Vec<(String, String)>,
    /// [`Schema::fingerprint`] before the load.
    expected: Vec<String>,
}

impl Schema {
    async fn read(conn: &mut PgConnection) -> sqlx::Result<Self> {
        let tables = BULK_TABLES.map(str::to_owned).to_vec();
        let constraints: Vec<(String, String, String, String)> = sqlx::query_as(
            "select c.conrelid::regclass::text, c.conname::text, c.contype::text,
                    pg_get_constraintdef(c.oid)
             from pg_constraint c
             where c.contype in ('p', 'u', 'f')
               and (c.conrelid = any($1::text[]::regclass[])
                    or c.confrelid = any($1::text[]::regclass[]))
             order by 1, 2",
        )
        .bind(&tables)
        .fetch_all(&mut *conn)
        .await?;
        let indexes = sqlx::query_as(
            "select ci.relname::text, pg_get_indexdef(i.indexrelid)
             from pg_index i join pg_class ci on ci.oid = i.indexrelid
             where i.indrelid = any($1::text[]::regclass[])
               and not exists (select from pg_constraint c
                               where c.conindid = i.indexrelid and c.conrelid = i.indrelid
                                 and c.contype in ('p', 'u', 'x'))
             order by 1",
        )
        .bind(&tables)
        .fetch_all(&mut *conn)
        .await?;
        let (foreign, keys): (Vec<_>, Vec<_>) = constraints.into_iter().partition(|c| c.2 == "f");
        let strip = |c: (String, String, String, String)| (c.0, c.1, c.3);
        Ok(Self {
            keys: keys.into_iter().map(strip).collect(),
            foreign: foreign.into_iter().map(strip).collect(),
            indexes,
            expected: Self::fingerprint(conn).await?,
        })
    }

    /// Every constraint and index of the bulk tables with its definition and whether it is
    /// validated: what must be the same before and after the load.
    async fn fingerprint(conn: &mut PgConnection) -> sqlx::Result<Vec<String>> {
        let tables = BULK_TABLES.map(str::to_owned).to_vec();
        sqlx::query_scalar(
            "select c.conrelid::regclass::text || ' ' || c.conname || ' ' || c.contype::text || ' '
                    || pg_get_constraintdef(c.oid) || ' ' || c.convalidated
             from pg_constraint c
             where c.conrelid = any($1::text[]::regclass[])
                or c.confrelid = any($1::text[]::regclass[])
             union all
             select indexdef from pg_indexes
             where schemaname = 'public' and tablename = any($1)
             order by 1",
        )
        .bind(&tables)
        .fetch_all(conn)
        .await
    }

    async fn drop(&self, conn: &mut PgConnection) -> sqlx::Result<()> {
        let mut sql = String::new();
        for (table, name, _) in self.foreign.iter().chain(&self.keys) {
            let _ = writeln!(sql, r#"alter table {table} drop constraint "{name}";"#);
        }
        for (name, _) in &self.indexes {
            let _ = writeln!(sql, r#"drop index "{name}";"#);
        }
        execute_built(conn, sql).await
    }

    /// Builds keys and indexes again (with `more` jobs alongside), then foreign keys: added
    /// without a check, which is instant, then validated in parallel, which only reads.
    async fn restore(
        &self,
        options: &PgConnectOptions,
        conn: &mut PgConnection,
    ) -> anyhow::Result<()> {
        let mut jobs: Vec<Job> = self
            .keys
            .iter()
            .map(|(table, name, def)| {
                Job::Sql(format!(
                    r#"alter table {table} add constraint "{name}" {def}"#
                ))
            })
            .collect();
        jobs.extend(self.indexes.iter().map(|(_, def)| Job::Sql(def.clone())));
        run_parallel(options, jobs).await?;
        let mut sql = String::new();
        for (table, name, def) in &self.foreign {
            let _ = writeln!(
                sql,
                r#"alter table {table} add constraint "{name}" {def} not valid;"#
            );
        }
        execute_built(&mut *conn, sql).await?;
        let validate = self
            .foreign
            .iter()
            .map(|(table, name, _)| {
                Job::Sql(format!(
                    r#"alter table {table} validate constraint "{name}""#
                ))
            })
            .collect();
        run_parallel(options, validate).await?;
        let after = Self::fingerprint(conn).await?;
        let differ: Vec<&String> = after
            .iter()
            .filter(|line| !self.expected.contains(line))
            .chain(self.expected.iter().filter(|line| !after.contains(line)))
            .collect();
        anyhow::ensure!(
            differ.is_empty(),
            "the issue tables differ from the migrations after the load: {differ:?}"
        );
        Ok(())
    }
}

/// A statement of the generator, run by [`run_parallel`].
#[derive(Debug, Clone)]
enum Job {
    /// Built from the benchmark's own SQL files and table names, never from input.
    Sql(String),
}

/// Runs jobs on several connections at once.
async fn run_parallel(options: &PgConnectOptions, jobs: Vec<Job>) -> anyhow::Result<()> {
    let jobs = Arc::new(Mutex::new(VecDeque::from(jobs)));
    let mut workers = tokio::task::JoinSet::new();
    for _ in 0..WORKERS {
        let (options, jobs) = (options.clone(), Arc::clone(&jobs));
        workers.spawn(async move {
            let mut conn = connect(&options).await?;
            loop {
                let next = jobs.lock().ok().and_then(|mut queue| queue.pop_front());
                match next {
                    None => break,
                    Some(Job::Sql(sql)) => execute_built(&mut conn, sql.clone())
                        .await
                        .with_context(|| format!("failed: {sql}"))?,
                }
            }
            conn.close().await?;
            Ok::<_, anyhow::Error>(())
        });
    }
    while let Some(done) = workers.join_next().await {
        done??;
    }
    Ok(())
}

/// Tables the dataset fills.
fn tables() -> Vec<String> {
    REAL_TABLES.map(str::to_owned).to_vec()
}

/// Rows per table when the dataset was generated, the largest first.
pub async fn row_counts(conn: &mut PgConnection) -> sqlx::Result<Vec<(String, i64)>> {
    sqlx::query_as(
        "select t.key, t.value::bigint from bench.dataset, jsonb_each_text(tables) t
         order by t.value::bigint desc, t.key",
    )
    .fetch_all(conn)
    .await
}

/// The custom role of reporters, role assignments and an API token for every user.
async fn access(conn: &mut PgConnection, plan: Plan) -> anyhow::Result<()> {
    let reporter = PermissionSet::from_keys(&["issue.create", "comment.create"]).closure();
    sqlx::query("insert into roles (name, permissions) values ('Reporter', $1)")
        .bind(
            reporter
                .keys()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )
        .execute(&mut *conn)
        .await?;
    execute(conn, ACCESS_SQL).await?;
    let users: Vec<i64> = sqlx::query_scalar("select id from users order by id")
        .fetch_all(&mut *conn)
        .await?;
    let hashes: Vec<Vec<u8>> = users
        .iter()
        .map(|user| Sha256::digest(api_token(plan.seed, *user)).to_vec())
        .collect();
    let scopes = Scopes::parse(&["tracker.read".to_owned(), "tracker.write".to_owned()])
        .map_err(|rejection| anyhow::anyhow!("{rejection:?}"))?
        .keys();
    sqlx::query(
        "insert into api_tokens (user_id, name, scopes, token_hash, created_by)
         select t.user_id, 'Benchmark', $3, t.hash, t.user_id
         from unnest($1::bigint[], $2::bytea[]) as t(user_id, hash)",
    )
    .bind(users)
    .bind(hashes)
    .bind(scopes)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The API token of `user` in the dataset of `seed`: derived, so the benchmark computes it again
/// instead of storing it; shaped as the server makes them (`service::secrets::token`).
pub fn api_token(seed: u64, user: i64) -> String {
    let mut hash = Sha256::new();
    hash.update(b"bench");
    hash.update(seed.to_le_bytes());
    hash.update(user.to_le_bytes());
    format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(hash.finalize()))
}

/// Personal tags: a few for every [`issues::TAGGERS`]-th user (the IDs of [`issues::tags_of`]).
fn tags_rows(model: &Model) -> CopyBuf {
    let mut rows = CopyBuf::new("copy tags (id, name, owner_id, color) from stdin");
    for (index, &user) in model.users.iter().enumerate() {
        for tag in issues::tags_of(index) {
            let word = model.vocabulary.at(1000 + (tag as usize) % 5000);
            rows.int(tag).text(word).int(user).int(tag % 12).end();
        }
    }
    rows
}

fn users_rows(plan: Plan) -> CopyBuf {
    let vocabulary = Vocabulary::new(plan.seed);
    let mut rng = Rng::for_item(plan.seed, "users", 0);
    let mut rows =
        CopyBuf::new("copy users (username, full_name, email, created_at, updated_at) from stdin");
    let name = |rng: &mut Rng| {
        let word = vocabulary.at(500 + rng.index(10_000));
        let mut chars = word.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect::<String>()
        })
    };
    for n in 1..=plan.users() {
        let username = format!("user{n:05}");
        let full_name = format!("{} {}", name(&mut rng), name(&mut rng));
        let created = END - rng.between(30, 3000) * 86_400;
        rows.text(&username)
            .text(&full_name)
            .text(&format!("{username}@example.test"))
            .time(created)
            .time(created)
            .end();
    }
    rows
}

/// Value sets and their values, projects with their keys and teams, groups of group fields,
/// fields and which project uses which field, in the order their foreign keys need. Rows keep the
/// model's IDs, which the issue tables refer to; sets of people hold the project team or, without
/// a project, all members (the built-in group `all_members`).
fn metadata_rows(model: &Model, all_members: i64) -> [CopyBuf; 14] {
    let mut sets = CopyBuf::new("copy value_sets (id, name, kind) from stdin");
    let mut values =
        CopyBuf::new("copy field_values (id, set_id, name, rank, resolved) from stdin");
    let mut groups = CopyBuf::new("copy groups (id, name, team_of) from stdin");
    let mut set_groups = CopyBuf::new("copy value_set_groups (set_id, group_id) from stdin");
    for set in &model.sets {
        let Some(kind) = set.kind.set_kind() else {
            // The list of a group field: its values are groups.
            for group in &set.values {
                groups.int(group.id).text(&group.name).null().end();
            }
            continue;
        };
        sets.int(set.id.into()).text(&set.name).text(kind).end();
        for (value, rank) in set.values.iter().zip(rank::sequence(set.values.len())) {
            values
                .int(value.id)
                .int(set.id.into())
                .text(&value.name)
                .text(&rank)
                .bool(value.resolved)
                .end();
        }
        if set.kind == Kind::User {
            let team = set.team.and_then(|p| model.projects.get(p));
            let group = team.map_or(all_members, |project| project.team_group);
            set_groups.int(set.id.into()).int(group).end();
        }
    }

    let mut projects = CopyBuf::new(
        "copy projects (id, key, name, template, owner_id, logo_color, next_number, created_by) \
         from stdin",
    );
    let mut keys = CopyBuf::new("copy project_keys (key, project_id) from stdin");
    let mut members = CopyBuf::new("copy group_members (group_id, user_id) from stdin");
    let mut attached = CopyBuf::new(
        "copy project_fields (project_id, field_id, set_id, rank, can_be_empty) from stdin",
    );
    let set_id = |set: Option<usize>| set.and_then(|s| model.sets.get(s)).map(|s| i64::from(s.id));
    for (project, color) in model.projects.iter().zip((0..8).cycle()) {
        let id = i64::from(project.id);
        let owner = project
            .team
            .first()
            .or(model.users.first())
            .copied()
            .unwrap_or(1);
        projects
            .int(id)
            .text(&project.key)
            .text(&project.name)
            .text("blank")
            .int(owner)
            .int(color)
            .int(i64::from(project.issues) + 1)
            .int(owner)
            .end();
        keys.text(&project.key).int(id).end();
        groups
            .int(project.team_group)
            .text(&project.name)
            .int(id)
            .end();
        for &user in &project.team {
            members.int(project.team_group).int(user).end();
        }
        for (field, rank) in project
            .fields
            .iter()
            .zip(rank::sequence(project.fields.len()))
        {
            let def = model.fields.get(field.field);
            attached
                .int(id)
                .int(def.map_or(0, |f| f.id.into()))
                .opt_int(set_id(field.stored_set))
                .text(&rank)
                .bool(def.is_none_or(|f| f.kind != Kind::State))
                .end();
        }
    }

    let mut fields =
        CopyBuf::new("copy fields (id, name, kind, multi, rank, set_id, can_be_empty) from stdin");
    for (field, rank) in model.fields.iter().zip(rank::sequence(model.fields.len())) {
        fields
            .int(field.id.into())
            .text(&field.name)
            .text(field.kind.name())
            .bool(field.multi)
            .text(&rank)
            .opt_int(set_id(field.set))
            .bool(field.kind != Kind::State)
            .end();
    }
    let [boards, board_projects, columns, sprints] = board_rows(model);
    [
        sets,
        values,
        projects,
        keys,
        groups,
        members,
        set_groups,
        fields,
        attached,
        rules_rows(model),
        boards,
        board_projects,
        columns,
        sprints,
    ]
}

/// The rule rows of step 1.9 (§3.14): the blank template's set for the twenty busiest projects —
/// the ones with boards, so hooks actually run on their write paths — and the scrum rule for a
/// few of them, as the settings tab could turn on: their card moves answer assign-on-move and
/// their chains walk under `scrum` (AUT-085, step 1.9b-3).
fn rules_rows(model: &Model) -> CopyBuf {
    const RULE_PROJECTS: usize = 20;
    const SCRUM_PROJECTS: usize = 3;
    let mut rows = CopyBuf::new("copy project_rules (project_id, rule) from stdin");
    for (index, project) in model.projects.iter().enumerate().take(RULE_PROJECTS) {
        for rule in rules::template_set(Template::Blank) {
            rows.int(project.id.into()).text(Rule::key(*rule)).end();
        }
        if index < SCRUM_PROJECTS {
            rows.int(project.id.into()).text(Rule::Scrum.key()).end();
        }
    }
    rows
}

/// The sprint boards of the busiest projects (`model::BoardPlan`): owned by the project's owner,
/// viewed by its team, a column per state and a sprint per value of the sprint field. Their cards
/// come with the issues.
fn board_rows(model: &Model) -> [CopyBuf; 4] {
    let mut boards = CopyBuf::new(
        "copy boards (id, name, owner_id, preset, sprints_on, column_field_id, view_groups) \
         from stdin",
    );
    let mut projects = CopyBuf::new("copy board_projects (board_id, project_id) from stdin");
    let mut columns = CopyBuf::new("copy board_columns (board_id, value_id, rank) from stdin");
    let mut sprints = CopyBuf::new("copy sprints (id, board_id, name) from stdin");
    for project in &model.projects {
        let Some(board) = project.board else { continue };
        let owner = project
            .team
            .first()
            .or(model.users.first())
            .copied()
            .unwrap_or(1);
        let field = model
            .fields
            .get(board.state_field)
            .map_or(0, |f| f.id.into());
        boards
            .int(board.id)
            .text(&format!("{} board", project.name))
            .int(owner)
            .text("scrum")
            .bool(true)
            .int(field)
            .ints(&[project.team_group])
            .end();
        projects.int(board.id).int(project.id.into()).end();
        let states = model
            .sets
            .get(board.state_set)
            .map_or(&[][..], |s| &s.values);
        // The preset's columns (BRD-003): every unresolved value in set order, then the first
        // resolved one — a board has no «Дубликат» column to drop a card into, where the
        // duplicates rule would refuse the move without a link (AUT-071 rule 5).
        let mut shown: Vec<&model::Value> = states.iter().filter(|v| !v.resolved).collect();
        shown.extend(states.iter().find(|v| v.resolved));
        for (value, rank) in shown.iter().zip(rank::sequence(shown.len())) {
            columns.int(board.id).int(value.id).text(&rank).end();
        }
        let values = model
            .sets
            .get(board.sprint_set)
            .map_or(&[][..], |s| &s.values);
        for (n, value) in values.iter().enumerate() {
            sprints
                .int(board.first_sprint + n as i64)
                .int(board.id)
                .text(&value.name)
                .end();
        }
    }
    [boards, projects, columns, sprints]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_plan() {
        let plan = Plan::new(1.0, SEED);
        assert_eq!(plan.issues(), 1_000_000);
        assert_eq!(plan.projects(), 200);
        assert_eq!(plan.users(), 10_000);
        assert_ne!(plan.fingerprint(), Plan::new(0.5, SEED).fingerprint());
        assert_eq!(plan.fingerprint(), Plan::new(1.0, SEED).fingerprint());
    }
}
