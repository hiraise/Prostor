//! Connection pool and migrations (ADM-048).

use std::str::FromStr;
use std::time::Duration;

use sqlx::migrate::{MigrateError, Migrator};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};

/// Migrations embedded into the binary (D-012) and applied on startup.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Longest a query of the pool may run. Long work belongs in jobs, which can raise the limit
/// for their own transaction with `set local statement_timeout`.
const STATEMENT_TIMEOUT: &str = "30s";
/// How long a request waits for a free connection before it fails.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
/// Advisory lock held by [`migrate`] (`trackmig` in ASCII).
const MIGRATION_LOCK: i64 = 0x7472_6163_6b6d_6967;

/// Parses a `postgres://` URL.
pub fn connect_options(url: &str) -> Result<PgConnectOptions, sqlx::Error> {
    Ok(PgConnectOptions::from_str(url)?.application_name("trackflow"))
}

/// A pool that connects on first use, so the server can start and report readiness
/// while the database is still unreachable.
pub fn pool(options: &PgConnectOptions, max_connections: u32) -> PgPool {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(0)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_lazy_with(
            options
                .clone()
                .options([("statement_timeout", STATEMENT_TIMEOUT)]),
        )
}

/// Whether the database answers within `timeout`.
///
/// A pool whose connections are all busy counts as reachable: under heavy load every node would
/// otherwise report itself unavailable at once and the load balancer would drop all of them.
pub async fn ping(pool: &PgPool, timeout: Duration) -> bool {
    let check = async {
        let mut conn = pool.acquire().await?;
        conn.ping().await
    };
    match tokio::time::timeout(timeout, check).await {
        Ok(Ok(())) => true,
        Ok(Err(sqlx::Error::PoolTimedOut)) | Err(_) => all_connections_busy(pool),
        Ok(Err(_)) => false,
    }
}

fn all_connections_busy(pool: &PgPool) -> bool {
    pool.size() >= pool.options().get_max_connections() && pool.num_idle() == 0
}

/// What [`migrate`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// Migrations applied by this run, as `0001_foundation`.
    pub applied: Vec<String>,
    /// The newest migration in the database now.
    pub latest: Option<i64>,
}

/// Applies pending migrations over a dedicated connection, without the pool's statement timeout.
///
/// Each migration runs in its own transaction and is recorded with time, checksum and duration in
/// `_sqlx_migrations`; a failed one leaves the database at the last good migration. Nodes starting
/// at once wait for each other on an advisory lock. A database that has a migration this build does
/// not know (a newer version ran on it) or a changed migration is refused: no downgrades.
pub async fn migrate(options: &PgConnectOptions) -> Result<MigrationReport, MigrateError> {
    let mut conn = PgConnection::connect_with(options).await?;
    // Our own lock around reading what is applied and applying the rest, so each node reports
    // only what it applied itself. The migrator takes its own lock inside as well.
    let result = async {
        sqlx::query!("select from pg_advisory_lock($1)", MIGRATION_LOCK)
            .execute(&mut conn)
            .await?;
        let before = applied_versions(&mut conn).await?;
        // `run_direct` instead of `run`: the future of `run` over a connection is not `Send`.
        MIGRATOR.run_direct(None, &mut conn, false).await?;
        Ok::<_, MigrateError>(before)
    }
    .await;
    // Closing releases both locks, also after a failed run.
    let _ = conn.close().await;
    let before = result?;

    let applied = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration() && !before.contains(&m.version))
        .map(|m| format!("{:04}_{}", m.version, m.description.replace(' ', "_")))
        .collect();
    let latest = MIGRATOR.iter().map(|m| m.version).max();
    Ok(MigrationReport { applied, latest })
}

async fn applied_versions(conn: &mut PgConnection) -> Result<Vec<i64>, sqlx::Error> {
    let exists =
        sqlx::query_scalar!(r#"select to_regclass('_sqlx_migrations') is not null as "exists!""#)
            .fetch_one(&mut *conn)
            .await?;
    if !exists {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!("select version from _sqlx_migrations where success")
        .fetch_all(conn)
        .await
}
