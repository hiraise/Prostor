//! Test support: a fresh, migrated PostgreSQL database for every test (conventions.md §4).
//!
//! The first test of a run builds a template database with all migrations applied. Its name holds
//! a hash of the migrations, so changing them builds a new template. Every test then clones the
//! template — a file copy inside PostgreSQL, a few milliseconds — and drops its clone at the end.
//! Tests run in parallel processes (nextest); building, cloning and removing templates happen
//! under one advisory lock, so no process removes a template another is cloning. Templates of
//! other migration sets (another branch or worktree) are removed only when they are stale.
//!
//! Tests that start a node or a job runner use `#[tokio::test(flavor = "multi_thread")]`. When a
//! test ends, its node may still be opening a connection; dropping the database blocks the thread
//! it runs on, and on the single-thread runtime that connection then never finishes its handshake.
//! PostgreSQL makes every `DROP DATABASE` on the server wait for such half-open connections (up to
//! `authentication_timeout`, a minute), so one stuck test would slow down all others.
//!
//! The server is the one in `DATABASE_URL` (default: the dev database of `make db-up`); its
//! database name is ignored. `TASKMASTER_TEST_KEEP_DB=1` keeps the databases for inspection.

// A test helper that cannot set up its database must fail the test right there.
#![allow(clippy::expect_used, clippy::panic)]

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::{AssertSqlSafe, Connection, Executor, PgConnection, PgPool};
use taskmaster_store::{MIGRATOR, PgConnectOptions};

const DEFAULT_SERVER: &str = "postgres://taskmaster:taskmaster@127.0.0.1:5433/postgres";
const HINT: &str = "cannot reach the test PostgreSQL server; is it running (`make db-up`)?";
/// Template names: `taskmaster_tpl_<migrations hash>_<unix seconds>`.
const TEMPLATE_PREFIX: &str = "taskmaster_tpl_";
/// Test database names: `taskmaster_t_<unix seconds>_<random>`.
const TEST_PREFIX: &str = "taskmaster_t_";
/// Serializes template work across test processes.
const LOCK_KEY: i64 = 0x7472_6163_6b66_6c77;
/// Test databases of crashed runs and templates of other migration sets older than this go away.
const STALE_AFTER: Duration = Duration::from_secs(3600);

/// A database that exists for one test.
pub struct TestDb {
    pool: PgPool,
    options: PgConnectOptions,
    name: String,
    server: PgConnectOptions,
}

impl TestDb {
    /// A new database with all migrations applied.
    pub async fn new() -> Self {
        let server = server_options();
        let mut admin = admin_connection(&server).await;
        run(&mut admin, format!("select pg_advisory_lock({LOCK_KEY})")).await;
        let template = ensure_template(&mut admin, &server).await;
        let name = create_database(&mut admin, &template).await;
        remove_stale(&mut admin).await;
        run(&mut admin, format!("select pg_advisory_unlock({LOCK_KEY})")).await;
        let _ = admin.close().await;
        Self::open(server, name)
    }

    /// A new database without any tables, for tests of the migrations themselves.
    pub async fn empty() -> Self {
        let server = server_options();
        let mut admin = admin_connection(&server).await;
        let name = create_database(&mut admin, "template0").await;
        let _ = admin.close().await;
        Self::open(server, name)
    }

    fn open(server: PgConnectOptions, name: String) -> Self {
        let options = server.clone().database(&name);
        // Small: tests run in parallel processes against one server.
        let pool = taskmaster_store::pool(&options, 6);
        Self {
            pool,
            options,
            name,
            server,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Connection options of this database, for code that opens its own connections.
    pub fn options(&self) -> &PgConnectOptions {
        &self.options
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        if std::env::var_os("TASKMASTER_TEST_KEEP_DB").is_some() {
            return;
        }
        let server = self.server.clone();
        let name = std::mem::take(&mut self.name);
        // Drop cannot await, and the test's runtime may be busy shutting down: use a thread
        // with its own small runtime. `with (force)` closes the pool's connections.
        let dropped = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            runtime.block_on(async {
                let mut admin = PgConnection::connect_with(&server.database("postgres"))
                    .await
                    .ok()?;
                let dropped = admin
                    .execute(AssertSqlSafe(format!(
                        r#"drop database if exists "{name}" with (force)"#
                    )))
                    .await
                    .ok();
                let _ = admin.close().await;
                dropped
            })
        })
        .join();
        // A leftover database is removed by a later run (STALE_AFTER); never fail a test here.
        let _ = dropped;
    }
}

fn server_options() -> PgConnectOptions {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_SERVER.to_owned());
    taskmaster_store::connect_options(&url).expect("DATABASE_URL is not a valid postgres:// URL")
}

async fn admin_connection(server: &PgConnectOptions) -> PgConnection {
    PgConnection::connect_with(&server.clone().database("postgres"))
        .await
        .unwrap_or_else(|error| panic!("{HINT} {error}"))
}

/// Runs one statement. SQL built here only ever contains numbers and database names made by this
/// crate, which is why `AssertSqlSafe` is fine.
async fn run(admin: &mut PgConnection, sql: String) {
    if let Err(error) = admin.execute(AssertSqlSafe(sql.clone())).await {
        panic!("test database setup failed on `{sql}`: {error}");
    }
}

async fn database_names(admin: &mut PgConnection, prefix: &str) -> Vec<(String, bool)> {
    sqlx::query_as("select datname, datistemplate from pg_database where starts_with(datname, $1)")
        .bind(prefix)
        .fetch_all(&mut *admin)
        .await
        .expect("cannot list databases")
}

async fn create_database(admin: &mut PgConnection, template: &str) -> String {
    let name = format!(
        "{TEST_PREFIX}{}_{}",
        unix_seconds(),
        uuid::Uuid::new_v4().simple()
    );
    run(
        admin,
        format!(r#"create database "{name}" template "{template}""#),
    )
    .await;
    name
}

/// Name of the template for the current migrations; builds it if needed. Call under the lock.
async fn ensure_template(admin: &mut PgConnection, server: &PgConnectOptions) -> String {
    let prefix = format!("{TEMPLATE_PREFIX}{:016x}_", migrations_hash());
    let mine = database_names(admin, &prefix).await;
    // `datistemplate` is set only after the migrations succeeded, so it marks a complete template.
    if let Some((name, _)) = mine.iter().find(|(_, complete)| *complete) {
        return name.clone();
    }
    for (half_built, _) in mine {
        drop_database(admin, &half_built).await;
    }

    let name = format!("{prefix}{}", unix_seconds());
    run(admin, format!(r#"create database "{name}""#)).await;
    taskmaster_store::migrate(&server.clone().database(&name))
        .await
        .unwrap_or_else(|error| panic!("migrations fail on an empty database: {error}"));
    run(
        admin,
        format!(r#"alter database "{name}" is_template true"#),
    )
    .await;
    name
}

fn migrations_hash() -> u64 {
    let mut hasher = DefaultHasher::new();
    for migration in MIGRATOR.iter() {
        migration.version.hash(&mut hasher);
        migration.checksum.hash(&mut hasher);
    }
    hasher.finish()
}

/// Removes test databases of crashed runs and templates of other migration sets, once stale.
/// Call under the lock.
async fn remove_stale(admin: &mut PgConnection) {
    let current = format!("{TEMPLATE_PREFIX}{:016x}_", migrations_hash());
    let now = unix_seconds();
    let mut stale = Vec::new();
    for prefix in [TEST_PREFIX, TEMPLATE_PREFIX] {
        for (name, _) in database_names(admin, prefix).await {
            if name.starts_with(&current) {
                continue;
            }
            // Test names have the time first, template names last.
            let rest = name.trim_start_matches(prefix);
            let seconds = if prefix == TEST_PREFIX {
                rest.split('_').next()
            } else {
                rest.rsplit('_').next()
            };
            let created = seconds.and_then(|seconds| seconds.parse::<u64>().ok());
            if created.is_none_or(|created| now.saturating_sub(created) > STALE_AFTER.as_secs()) {
                stale.push(name);
            }
        }
    }
    for name in stale {
        drop_database(admin, &name).await;
    }
}

async fn drop_database(admin: &mut PgConnection, name: &str) {
    // A template must be unmarked before it can be dropped.
    run(
        admin,
        format!(r#"alter database "{name}" is_template false"#),
    )
    .await;
    run(
        admin,
        format!(r#"drop database if exists "{name}" with (force)"#),
    )
    .await;
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}
