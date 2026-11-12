//! The report: the dataset, the database it ran on, each scenario against its budget, the SQL
//! breakdown when asked for, and the benchmark's own time against its budget (D-032).

use std::io::{self, Write};
use std::time::Duration;

use sqlx::PgConnection;

use crate::runner::{Measurement, RunOptions, Stats};
use crate::scenarios::live::{self, LiveMeasurement};

/// The benchmark's own time budget, without a cold build (D-032).
pub const TIME_BUDGET: Duration = Duration::from_secs(3 * 60);
/// The same when the run generated the dataset first.
pub const TIME_BUDGET_GENERATING: Duration = Duration::from_secs(5 * 60);
/// Statements listed per scenario in the breakdown.
const BREAKDOWN_LINES: usize = 12;

/// How long the benchmark took, by phase.
#[derive(Debug, Clone, Default)]
pub struct Timing {
    pub phases: Vec<(String, Duration)>,
    /// Whether the run generated the dataset.
    pub generated: bool,
}

impl Timing {
    pub fn total(&self) -> Duration {
        self.phases.iter().map(|p| p.1).sum()
    }

    pub fn budget(&self) -> Duration {
        if self.generated {
            TIME_BUDGET_GENERATING
        } else {
            TIME_BUDGET
        }
    }

    pub fn within_budget(&self) -> bool {
        self.total() <= self.budget()
    }

    /// The phase that took longest.
    pub fn longest(&self) -> Option<&(String, Duration)> {
        self.phases.iter().max_by_key(|p| p.1)
    }
}

/// What the measurements ran on.
#[derive(Debug, Clone)]
pub struct Environment {
    pub scale: f64,
    pub seed: i64,
    pub generated_in: f64,
    pub tables: Vec<(String, i64)>,
    pub server: String,
    pub settings: Vec<(String, String)>,
    pub size: String,
    /// Median `select 1` round trip, back to back and after a pause: in a VM (Docker on macOS)
    /// the second is several times the first, and a change of many queries at a slow rate pays
    /// it on each (step 1.6, 1.6-5).
    pub round_trip: (Duration, Duration),
}

/// The pause before each probe of the idle round trip: the gap between a team's changes is far
/// longer, but this is enough for a VM to idle.
const ROUND_TRIP_PAUSE: Duration = Duration::from_millis(20);

/// The median of 50 `select 1` round trips, each after `pause`.
async fn round_trip(conn: &mut PgConnection, pause: Duration) -> sqlx::Result<Duration> {
    let mut times = Vec::with_capacity(50);
    for _ in 0..50 {
        if !pause.is_zero() {
            tokio::time::sleep(pause).await;
        }
        let started = std::time::Instant::now();
        sqlx::query("select 1").execute(&mut *conn).await?;
        times.push(started.elapsed());
    }
    times.sort_unstable();
    Ok(times[times.len() / 2])
}

impl Environment {
    pub async fn read(conn: &mut PgConnection) -> sqlx::Result<Self> {
        let (scale, seed, generated_in): (f64, i64, f64) =
            sqlx::query_as("select scale, seed, seconds from bench.dataset")
                .fetch_one(&mut *conn)
                .await?;
        let tables = crate::dataset::row_counts(&mut *conn).await?;
        let server = sqlx::query_scalar("show server_version")
            .fetch_one(&mut *conn)
            .await?;
        let settings = sqlx::query_as(
            "select name, current_setting(name) from pg_settings
             where name in ('shared_buffers', 'effective_cache_size', 'work_mem', 'jit',
                            'max_parallel_workers_per_gather', 'random_page_cost')
             order by name",
        )
        .fetch_all(&mut *conn)
        .await?;
        let size =
            sqlx::query_scalar("select pg_size_pretty(pg_database_size(current_database()))")
                .fetch_one(&mut *conn)
                .await?;
        let round_trip = (
            round_trip(&mut *conn, Duration::ZERO).await?,
            round_trip(&mut *conn, ROUND_TRIP_PAUSE).await?,
        );
        Ok(Self {
            round_trip,
            scale,
            seed,
            generated_in,
            tables,
            server,
            settings,
            size,
        })
    }

    fn rows(&self, table: &str) -> i64 {
        self.tables.iter().find(|t| t.0 == table).map_or(0, |t| t.1)
    }

    /// Rows of all field value tables.
    fn field_values(&self) -> i64 {
        self.tables
            .iter()
            .filter(|t| t.0.starts_with("issue_") && t.0.ends_with("_values"))
            .map(|t| t.1)
            .sum()
    }
}

/// Writes the report; true when every scenario and the benchmark itself are within their budgets.
pub fn write(
    out: &mut impl Write,
    env: &Environment,
    run: &RunOptions,
    measurements: &[Measurement],
    live: Option<&LiveMeasurement>,
    timing: &Timing,
) -> io::Result<bool> {
    writeln!(
        out,
        "Dataset ×{} (seed {}, generated in {})",
        env.scale,
        env.seed,
        minutes(env.generated_in)
    )?;
    writeln!(
        out,
        "  {} issues · {} projects · {} users · {} fields · {} field values · {} history entries",
        count(env.rows("issues")),
        count(env.rows("projects")),
        count(env.rows("users")),
        count(env.rows("fields")),
        count(env.field_values()),
        count(env.rows("issue_history")),
    )?;
    let settings: Vec<String> = env
        .settings
        .iter()
        .map(|(name, value)| format!("{name} {value}"))
        .collect();
    writeln!(
        out,
        "PostgreSQL {} · database {} · {}",
        env.server,
        env.size,
        settings.join(" · ")
    )?;
    writeln!(
        out,
        "Database round trip (select 1, median): {:.2} ms back to back, {:.2} ms after a {} ms pause",
        env.round_trip.0.as_secs_f64() * 1e3,
        env.round_trip.1.as_secs_f64() * 1e3,
        ROUND_TRIP_PAUSE.as_millis()
    )?;
    writeln!(
        out,
        "{} client(s) · {} operations per scenario, measured after {} warm-up pass(es) over them · server time from the client",
        run.clients, run.iterations, run.warmup
    )?;
    writeln!(
        out,
        "[http]: whole requests to a node in this process · [sql]: the server's queries, until the endpoint exists\n"
    )?;

    writeln!(
        out,
        "{:<48} {:>8} {:>9} {:>9} {:>9} {:>7}",
        "Scenario", "Budget", "p50", "p95", "max", "rows"
    )?;
    let mut all_within = true;
    for m in measurements {
        let within = m.within_budget();
        all_within &= within || m.scenario.known_miss().is_some();
        let gate = if m.scenario.gate() { " (D-022)" } else { "" };
        let title = format!("{}{gate} {}", m.scenario.title(), m.scenario.tag());
        line(out, &title, Some(m.scenario.budget()), &m.overall)?;
        let mixed = !matches!(m.scenario.tag(), "[http]" | "[sql]");
        if m.variants.len() > 1 {
            for (index, (name, stats)) in m.variants.iter().enumerate() {
                let tag = if mixed {
                    format!(" {}", m.scenario.mode(index).tag())
                } else {
                    String::new()
                };
                line(out, &format!("  · {name}{tag}"), None, stats)?;
            }
        }
        match (within, m.scenario.known_miss()) {
            (true, _) => {}
            (false, None) => writeln!(out, "  ^ over budget")?,
            (false, Some(reason)) => writeln!(out, "  ^ over budget, known: {reason}")?,
        }
    }
    if let Some(m) = live {
        all_within &= m.within_budget();
        live_lines(out, m)?;
    }
    for m in measurements.iter().filter(|m| !m.statements.is_empty()) {
        breakdown(out, m)?;
    }
    writeln!(
        out,
        "\n{}",
        if all_within {
            "Every scenario is within its budget, apart from known misses."
        } else {
            "Some scenarios are over budget."
        }
    )?;
    let in_time = timing.within_budget();
    writeln!(
        out,
        "Bench time: {} (budget {}{})",
        minutes(timing.total().as_secs_f64()),
        minutes(timing.budget().as_secs_f64()),
        if timing.generated {
            ", with generation"
        } else {
            ""
        }
    )?;
    if !in_time && let Some((phase, took)) = timing.longest() {
        writeln!(
            out,
            "  ^ over budget; the longest phase: {phase}, {}",
            minutes(took.as_secs_f64())
        )?;
    }
    Ok(all_within && in_time)
}

/// The live scenario, one block per phase; rows are subscribers per change.
fn live_lines(out: &mut impl Write, m: &LiveMeasurement) -> io::Result<()> {
    let team = &m.team;
    let load = format!("{} sockets, {}/s", count(team.sockets as i64), live::RATE);
    line(
        out,
        &format!("Live delivery, team: {load} [http+ws]"),
        Some(live::DELIVERY_BUDGET),
        &team.delivery,
    )?;
    line(
        out,
        &format!("Field change commit, team: {load} [http]"),
        Some(live::CHANGE_BUDGET),
        &team.change,
    )?;
    let mut over = team.delivery.p95 > live::DELIVERY_BUDGET
        || team.change.p95 > live::CHANGE_BUDGET
        || team.missed > 0;
    if let Some(capacity) = &m.capacity {
        let load = format!(
            "{} sockets, {}/s",
            count(capacity.sockets as i64),
            live::RATE
        );
        line(
            out,
            &format!("Live delivery, capacity: {load} [http+ws]"),
            Some(live::DELIVERY_BUDGET),
            &capacity.delivery,
        )?;
        line(
            out,
            &format!("Field change, capacity, not a budget: {load} [http]"),
            None,
            &capacity.change,
        )?;
        over |= capacity.delivery.p95 > live::DELIVERY_BUDGET || capacity.missed > 0;
    }
    let memory = m.memory_per_connection.map_or_else(
        || "memory not read".to_owned(),
        |bytes| {
            format!(
                "node's resident memory +{:.0} KiB per socket",
                bytes / 1024.0
            )
        },
    );
    writeln!(
        out,
        "  {} of {} sockets opened in {} · {memory}",
        count(m.opened as i64),
        count(m.requested as i64),
        minutes(m.connecting.as_secs_f64()),
    )?;
    let missed = team.missed + m.capacity.as_ref().map_or(0, |c| c.missed);
    if missed > 0 {
        writeln!(out, "  ^ {missed} deliveries missing after 5 s")?;
    }
    if m.opened < m.requested {
        over = true;
        writeln!(
            out,
            "  ^ fewer sockets than the budget's: {}",
            m.failure.as_deref().unwrap_or("no reason")
        )?;
    }
    if over {
        writeln!(out, "  ^ over budget")?;
    }
    Ok(())
}

/// The statements of a scenario by time per operation, against the operation's own time.
fn breakdown(out: &mut impl Write, m: &Measurement) -> io::Result<()> {
    let per_op: Duration = m.statements.iter().map(|s| s.time).sum();
    let calls: f64 = m.statements.iter().map(|s| s.calls).sum();
    writeln!(
        out,
        "\nSQL of «{}»: {calls:.1} statements, {} per operation (median operation {})",
        m.scenario.title(),
        ms(per_op),
        ms(m.overall.p50),
    )?;
    writeln!(out, "{:>9} {:>6}  statement", "per op", "calls")?;
    for statement in m.statements.iter().take(BREAKDOWN_LINES) {
        let text: String = statement.text.chars().take(110).collect();
        writeln!(
            out,
            "{:>9} {:>6.1}  {text}",
            ms(statement.time),
            statement.calls
        )?;
    }
    if m.statements.len() > BREAKDOWN_LINES {
        let rest: Duration = m.statements[BREAKDOWN_LINES..].iter().map(|s| s.time).sum();
        writeln!(
            out,
            "{:>9}         {} more statements",
            ms(rest),
            m.statements.len() - BREAKDOWN_LINES
        )?;
    }
    Ok(())
}

fn line(
    out: &mut impl Write,
    title: &str,
    budget: Option<Duration>,
    stats: &Stats,
) -> io::Result<()> {
    let budget = budget.map_or_else(String::new, ms);
    writeln!(
        out,
        "{title:<48} {budget:>8} {:>9} {:>9} {:>9} {:>7.0}",
        ms(stats.p50),
        ms(stats.p95),
        ms(stats.max),
        stats.rows
    )
}

fn ms(duration: Duration) -> String {
    let ms = duration.as_secs_f64() * 1000.0;
    if ms >= 100.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{ms:.1} ms")
    }
}

fn minutes(seconds: f64) -> String {
    if seconds >= 60.0 {
        format!(
            "{:.0} min {:.0} s",
            (seconds / 60.0).floor(),
            seconds % 60.0
        )
    } else {
        format!("{seconds:.1} s")
    }
}

/// A count with thin groups: 1 000 000.
fn count(n: i64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_well() {
        assert_eq!(count(1_000_000), "1 000 000");
        assert_eq!(count(950), "950");
        assert_eq!(ms(Duration::from_micros(12_340)), "12.3 ms");
        assert_eq!(ms(Duration::from_millis(150)), "150 ms");
        assert_eq!(minutes(252.0), "4 min 12 s");
    }
}
