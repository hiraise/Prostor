//! Runs scenarios and measures them from one or more concurrent clients: requests over HTTP go to
//! one node in this process, the queries of the other scenarios run on a connection per client from
//! a pool set up as the server's.
//!
//! Each client first runs its operations unmeasured (a warm-up pass), then runs exactly the same
//! operations again and measures them. Every page they touch is then in memory, as on the servers of
//! the budget, where the whole dataset fits in RAM. Without the pass the numbers measure the machine:
//! a developer's computer swaps out the memory of an idle Docker VM within seconds.

use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::{Connection, PgConnection};
use taskmaster_store::PgConnectOptions;

use crate::breakdown::{self, Statement, Totals};
use crate::catalog::Catalog;
use crate::http::Node;
use crate::rng::Rng;
use crate::scenarios::Scenario;

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Measured operations per scenario.
    pub iterations: usize,
    /// Unmeasured passes over the same operations before the measured one.
    pub warmup: usize,
    /// Concurrent clients.
    pub clients: usize,
    /// Seed of the query parameters.
    pub seed: u64,
    /// Record the statements of every measured operation (one client only).
    pub breakdown: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            iterations: 200,
            warmup: 1,
            clients: 1,
            seed: 1,
            breakdown: false,
        }
    }
}

/// Times of a set of operations.
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub count: usize,
    pub p50: Duration,
    pub p95: Duration,
    pub max: Duration,
    /// Rows of the main result per operation.
    pub rows: f64,
}

impl Stats {
    pub fn of(samples: &[(Duration, usize)]) -> Self {
        let mut times: Vec<Duration> = samples.iter().map(|s| s.0).collect();
        times.sort_unstable();
        let rows = samples.iter().map(|s| s.1 as f64).sum::<f64>() / samples.len().max(1) as f64;
        Self {
            count: times.len(),
            p50: percentile(&times, 50),
            p95: percentile(&times, 95),
            max: times.last().copied().unwrap_or_default(),
            rows,
        }
    }
}

/// Nearest-rank percentile of sorted times.
fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied().unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct Measurement {
    pub scenario: Scenario,
    pub overall: Stats,
    pub variants: Vec<(&'static str, Stats)>,
    /// Statements per operation, with `--breakdown`.
    pub statements: Vec<Statement>,
    /// Wall time of the scenario, warm-up included.
    pub elapsed: Duration,
}

impl Measurement {
    pub fn within_budget(&self) -> bool {
        self.overall.p95 <= self.scenario.budget()
    }
}

pub async fn measure(
    options: &PgConnectOptions,
    catalog: &Arc<Catalog>,
    node: &Arc<Node>,
    scenario: Scenario,
    run: &RunOptions,
) -> anyhow::Result<Measurement> {
    let started = Instant::now();
    let clients = run.clients.max(1);
    let record = run.breakdown && clients == 1;
    if record {
        breakdown::install();
    }
    let variants = scenario.variants();
    // The server's pool: its statement timeout and connection settings.
    let pool = taskmaster_store::pool(options, u32::try_from(clients)?);
    let iterations = scenario.iterations(run);
    let per_client = iterations.div_ceil(clients);
    let (seed, warmup) = (run.seed, run.warmup);
    let mut tasks = tokio::task::JoinSet::new();
    for client in 0..clients {
        let (pool, catalog, node) = (pool.clone(), Arc::clone(catalog), Arc::clone(node));
        tasks.spawn(async move {
            let mut conn = pool.acquire().await?;
            let mut samples = Vec::with_capacity(per_client);
            let mut totals = Totals::default();
            for pass in 0..=warmup {
                // The same seed in every pass: the same operations with the same parameters.
                let mut rng = Rng::for_item(seed, scenario.key(), client as u64);
                let measured = pass == warmup;
                for n in 0..per_client {
                    let variant = (n * clients + client) % variants.len();
                    if record && measured {
                        breakdown::start();
                    }
                    let started = Instant::now();
                    let rows = scenario
                        .run(&catalog, &node, &mut conn, &mut rng, variant)
                        .await?;
                    let elapsed = started.elapsed();
                    if record && measured {
                        totals.add(breakdown::take());
                    }
                    if measured {
                        samples.push((variant, elapsed, rows));
                    }
                }
            }
            Ok::<_, anyhow::Error>((samples, totals))
        });
    }
    let mut samples = Vec::with_capacity(iterations);
    let mut statements = Vec::new();
    while let Some(done) = tasks.join_next().await {
        let (mine, totals) = done??;
        samples.extend(mine);
        if record {
            statements = totals.statements();
        }
    }
    pool.close().await;

    let all: Vec<(Duration, usize)> = samples.iter().map(|s| (s.1, s.2)).collect();
    let by_variant = variants
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let mine: Vec<(Duration, usize)> = samples
                .iter()
                .filter(|s| s.0 == index)
                .map(|s| (s.1, s.2))
                .collect();
            (*name, Stats::of(&mine))
        })
        .collect();
    Ok(Measurement {
        scenario,
        overall: Stats::of(&all),
        variants: by_variant,
        statements,
        elapsed: started.elapsed(),
    })
}

/// The executed plan of each variant's main query.
pub async fn explain(
    options: &PgConnectOptions,
    catalog: &Catalog,
    scenario: Scenario,
    seed: u64,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    let mut conn = PgConnection::connect_with(options).await?;
    let mut rng = Rng::for_item(seed, "explain", scenario as u64);
    let mut plans = Vec::new();
    for (index, name) in scenario.variants().iter().enumerate() {
        if let Some(query) = scenario.main_query(catalog, &mut rng, index) {
            plans.push((*name, query.explain(&mut conn).await?));
        }
    }
    conn.close().await?;
    Ok(plans)
}

/// Reads the dataset into PostgreSQL's buffers, as on a server that has been running for a while
/// (`deploy/compose.bench.yaml` makes them big enough).
pub async fn prewarm(options: &PgConnectOptions) -> anyhow::Result<()> {
    let mut conn = crate::dataset::connect(options).await?;
    sqlx::raw_sql("create extension if not exists pg_prewarm")
        .execute(&mut conn)
        .await?;
    sqlx::raw_sql(
        "select pg_prewarm(c.oid::regclass, 'buffer')
         from pg_class c join pg_namespace n on n.oid = c.relnamespace
         where c.relkind in ('r', 'i') and n.nspname = 'public'",
    )
    .execute(&mut conn)
    .await?;
    conn.close().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let samples: Vec<(Duration, usize)> = (1..=100)
            .map(|ms| (Duration::from_millis(ms), 50))
            .collect();
        let stats = Stats::of(&samples);
        assert_eq!(stats.p50, Duration::from_millis(50));
        assert_eq!(stats.p95, Duration::from_millis(95));
        assert_eq!(stats.max, Duration::from_millis(100));
        assert!((stats.rows - 50.0).abs() < f64::EPSILON);
        assert_eq!(Stats::of(&[]).p95, Duration::ZERO);
    }
}
