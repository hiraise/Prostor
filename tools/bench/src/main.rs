//! `prostor-bench`: generates the dataset when it is missing or outdated, runs the
//! load scenarios and prints the report against the performance budget (architecture.md §7).

use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use prostor_bench::breakdown;
use prostor_bench::catalog::Catalog;
use prostor_bench::dataset::{self, Plan};
use prostor_bench::http::Node;
use prostor_bench::report::{self, Environment, Timing};
use prostor_bench::runner::{self, RunOptions};
use prostor_bench::scenarios::Scenario;
use prostor_bench::scenarios::live::{self, LiveOptions};
use prostor_store::PgConnectOptions;
use sqlx::{AssertSqlSafe, Connection, PgConnection};

const DEFAULT_URL: &str = "postgres://prostor:prostor@127.0.0.1:5450/prostor_bench";

const USAGE: &str = "\
prostor-bench — the datasets and load scenarios of the performance budget

Usage: prostor-bench [options]          (or: make bench BENCH_ARGS=\"options\")

  --scale X        Dataset size: 0.1 is the target dataset (100k issues, the default),
                   1 the stress dataset (1M issues, `make bench-stress`).
  --seed N         Seed of the dataset and of the queries. Default 1.
  --regenerate     Generate the dataset again even when it is current.
  --only LIST      Scenarios, comma-separated: list, board, search, issue, change, live.
  --live-connections N
                   Sockets the live scenario opens. Default 5000.
  --iterations N   Measured operations per scenario. Default 200; the command scenario caps at
                   40 whatever N is (its bulk operations are whole jobs over ≈50 issues).
  --warmup N       Unmeasured passes over the operations before the measured one. Default 1.
  --clients N      Concurrent clients. Default 1.
  --explain        Print the executed plan of each main query instead of measuring
                   (queries of [sql] scenarios; --breakdown shows the [http] ones).
  --breakdown      List the SQL statements of each scenario by time per operation.
                   Needs --clients 1 (the default).

The database is PROSTOR_BENCH_DATABASE_URL, by default the one `make bench` starts:
  postgres://prostor:prostor@127.0.0.1:5450/prostor_bench
The dataset is generated there when it is missing or was made by another generator, schema,
scale or seed; that drops and recreates the database. Exit status 1: a scenario is over budget
and not a known miss (see the report), or the benchmark took longer than its own budget
(3 min, 5 min when it generated the dataset, plus the measured scenarios' time allowances —
the command scenario adds 4 min for its whole jobs).
";

#[derive(Debug)]
struct Args {
    plan: Plan,
    regenerate: bool,
    only: Vec<Scenario>,
    /// Run the live scenario, with this many sockets.
    live: Option<usize>,
    run: RunOptions,
    explain: bool,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Args>, String> {
    let mut parsed = Args {
        plan: Plan::new(dataset::TARGET_SCALE, dataset::SEED),
        regenerate: false,
        only: Scenario::ALL.to_vec(),
        live: Some(live::CONNECTIONS),
        run: RunOptions::default(),
        explain: false,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--help" | "-h" | "help" => return Ok(None),
            "--regenerate" => parsed.regenerate = true,
            "--explain" => parsed.explain = true,
            "--breakdown" => parsed.run.breakdown = true,
            "--scale" => {
                let scale: f64 = number(&value()?)?;
                if !(scale > 0.0 && scale <= 10.0) {
                    return Err("--scale must be above 0 and at most 10".to_owned());
                }
                parsed.plan.scale = scale;
            }
            "--seed" => parsed.plan.seed = number(&value()?)?,
            "--iterations" => parsed.run.iterations = number::<usize>(&value()?)?.max(1),
            "--warmup" => parsed.run.warmup = number(&value()?)?,
            "--clients" => parsed.run.clients = number::<usize>(&value()?)?.clamp(1, 64),
            "--only" => {
                let keys = value()?;
                let keys: Vec<&str> = keys.split(',').map(str::trim).collect();
                let connections = parsed.live.unwrap_or(live::CONNECTIONS);
                parsed.live = keys.contains(&"live").then_some(connections);
                parsed.only = keys
                    .iter()
                    .filter(|key| **key != "live")
                    .map(|key| Scenario::from_key(key).ok_or(format!("unknown scenario: {key}")))
                    .collect::<Result<_, _>>()?;
            }
            "--live-connections" => {
                let connections = number::<usize>(&value()?)?.clamp(1, 50_000);
                parsed.live = parsed.live.map(|_| connections);
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    if parsed.run.breakdown && parsed.run.clients > 1 {
        return Err("--breakdown needs --clients 1".to_owned());
    }
    parsed.run.seed = parsed.plan.seed;
    Ok(Some(parsed))
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse().map_err(|_| format!("not a number: {text}"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some(live::CLIENTS_FLAG) {
        return live::serve_clients().await;
    }
    let args = match parse(std::env::args().skip(1)) {
        Ok(Some(args)) => args,
        Ok(None) => return Ok(write!(io::stdout(), "{USAGE}")?),
        Err(problem) => {
            let _ = write!(io::stderr(), "{problem}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    // Errors of the node's requests land here: the node shares the server's router, whose error
    // layer logs through tracing. Nothing prints without RUST_LOG (the filter starts empty).
    // `install` also carries the statements layer of `--breakdown`; it is idempotent, and the
    // runner calls it again when it measures with the breakdown.
    breakdown::install();
    let url = std::env::var("PROSTOR_BENCH_DATABASE_URL").unwrap_or_else(|_| DEFAULT_URL.into());
    let options = prostor_store::connect_options(&url)?;

    let mut timing = Timing::default();
    let started = Instant::now();
    timing.generated = ensure_dataset(&options, args.plan, args.regenerate).await?;
    timing.phases.push((
        if timing.generated {
            "generation".to_owned()
        } else {
            "dataset check".to_owned()
        },
        started.elapsed(),
    ));
    let mut conn = PgConnection::connect_with(&options).await?;
    let catalog = Arc::new(Catalog::load(&mut conn).await?);

    if args.explain {
        let mut out = io::stdout();
        for scenario in &args.only {
            for (variant, plan) in
                runner::explain(&options, &catalog, *scenario, args.plan.seed).await?
            {
                writeln!(out, "── {} · {variant}\n{plan}\n", scenario.title())?;
            }
        }
        return Ok(());
    }

    progress("warming the cache");
    let started = Instant::now();
    runner::prewarm(&options).await?;
    // The live scenario's changes in flight need connections of their own.
    let clients = if args.live.is_some() {
        args.run.clients.max(live::CLIENTS)
    } else {
        args.run.clients
    };
    let node = Arc::new(Node::start(&options, clients).await?);
    timing
        .phases
        .push(("prewarm and node start".to_owned(), started.elapsed()));
    let mut measurements = Vec::new();
    timing.allowance = args.only.iter().map(|s| s.time_allowance()).sum();
    for scenario in &args.only {
        progress(&format!("measuring: {}", scenario.title()));
        let m = runner::measure(&options, &catalog, &node, *scenario, &args.run).await?;
        timing.phases.push((scenario.title().to_owned(), m.elapsed));
        measurements.push(m);
    }
    let mut live_measurement = None;
    if let Some(connections) = args.live {
        progress(&format!(
            "measuring: live delivery, {connections} connections"
        ));
        let live_options = LiveOptions {
            connections,
            changes: args.run.iterations,
            seed: args.run.seed,
            clients: std::env::current_exe().context("cannot find the bench program")?,
        };
        let m = live::measure(&catalog, &node, &live_options).await?;
        timing.phases.push(("live delivery".to_owned(), m.elapsed));
        live_measurement = Some(m);
    }
    if let Some(node) = Arc::into_inner(node) {
        node.stop().await;
    }
    let started = Instant::now();
    let env = Environment::read(&mut conn).await?;
    conn.close().await?;
    timing
        .phases
        .push(("report".to_owned(), started.elapsed().max(Duration::ZERO)));
    let within = report::write(
        &mut io::stdout(),
        &env,
        &args.run,
        &measurements,
        live_measurement.as_ref(),
        &timing,
    )?;
    if !within {
        std::process::exit(1);
    }
    Ok(())
}

/// Generates the dataset unless the database already has the one `plan` describes; true when it
/// generated.
async fn ensure_dataset(
    options: &PgConnectOptions,
    plan: Plan,
    regenerate: bool,
) -> anyhow::Result<bool> {
    let current = match PgConnection::connect_with(options).await {
        Ok(mut conn) => {
            let fingerprint = dataset::fingerprint(&mut conn).await?;
            conn.close().await?;
            fingerprint
        }
        Err(_) => None,
    };
    if !regenerate && current.as_deref() == Some(plan.fingerprint().as_str()) {
        return Ok(false);
    }
    progress(&format!(
        "generating the dataset ×{} (seed {}); at ×1 this takes minutes",
        plan.scale, plan.seed
    ));
    recreate_database(options).await?;
    prostor_store::migrate(options)
        .await
        .context("cannot migrate the benchmark database")?;
    let started = Instant::now();
    let log = move |stage: &str| {
        progress(&format!(
            "  {:>4.0} s  {stage}",
            started.elapsed().as_secs_f64()
        ))
    };
    let summary = dataset::generate(options, plan, &log).await?;
    progress(&format!("dataset ready in {:.0} s", summary.seconds));
    Ok(true)
}

async fn recreate_database(options: &PgConnectOptions) -> anyhow::Result<()> {
    let name = options
        .get_database()
        .context("the benchmark database URL names no database")?
        .replace('"', "");
    let mut admin = PgConnection::connect_with(&options.clone().database("postgres"))
        .await
        .context(
            "cannot reach the benchmark PostgreSQL; is it running (`make bench` starts it)?",
        )?;
    // The name comes from the URL the developer configured.
    for sql in [
        format!(r#"drop database if exists "{name}" with (force)"#),
        format!(r#"create database "{name}""#),
    ] {
        sqlx::raw_sql(AssertSqlSafe(sql))
            .execute(&mut admin)
            .await?;
    }
    admin.close().await?;
    Ok(())
}

fn progress(message: &str) {
    let _ = writeln!(io::stderr(), "{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Option<Args>, String> {
        parse(words.iter().map(|word| (*word).to_owned()))
    }

    #[test]
    fn options() {
        let args = parse_words(&["--scale", "0.1", "--only", "list,board", "--clients", "4"])
            .unwrap()
            .unwrap();
        assert!((args.plan.scale - 0.1).abs() < f64::EPSILON);
        assert_eq!(args.only, vec![Scenario::List, Scenario::Board]);
        assert_eq!(args.run.clients, 4);
        assert_eq!(args.live, None);
        let live = parse_words(&["--live-connections", "50", "--only", "live,issue"])
            .unwrap()
            .unwrap();
        assert_eq!(live.live, Some(50));
        assert_eq!(live.only, vec![Scenario::Open]);
        assert_eq!(parse_words(&[]).unwrap().unwrap().live, Some(5_000));
        assert!(parse_words(&["--help"]).unwrap().is_none());
        assert!(parse_words(&["--scale", "0"]).is_err());
        assert!(parse_words(&["--only", "nothing"]).is_err());
        assert!(parse_words(&["--iterations"]).is_err());
        assert!(
            parse_words(&["--breakdown"])
                .unwrap()
                .unwrap()
                .run
                .breakdown
        );
        assert!(parse_words(&["--breakdown", "--clients", "2"]).is_err());
    }
}
