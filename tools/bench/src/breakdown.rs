//! The SQL breakdown of `--breakdown` (D-032, D-035): the statements a request ran and their times,
//! read from the events sqlx writes for every statement (target `sqlx::query`, level debug, with
//! `elapsed_secs`). A request over HTTP is measured whole; this shows where its time went.
//!
//! Statements are recorded only between [`start`] and [`take`], so the benchmark runs one client
//! with it: the recording is shared by the whole process.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

/// Statements of the operation being recorded; `None` while nothing records.
static RECORDING: Mutex<Option<Vec<(String, Duration)>>> = Mutex::new(None);
static INSTALLED: OnceLock<()> = OnceLock::new();

/// The process's one subscriber: this statements layer plus the error logs of the node's router
/// (`RUST_LOG`, nothing by default). Both go in here — a second `try_init` anywhere else (it once
/// ran in `main` before the runner) silently loses, and with it every sqlx event.
pub fn install() {
    INSTALLED.get_or_init(|| {
        let _ = tracing_subscriber::registry()
            .with(Statements.with_filter(Targets::new().with_target("sqlx::query", Level::DEBUG)))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_filter(tracing_subscriber::EnvFilter::from_default_env()),
            )
            .try_init();
    });
}

/// Starts recording the statements of one operation.
pub fn start() {
    if let Ok(mut recording) = RECORDING.lock() {
        *recording = Some(Vec::new());
    }
}

/// Stops recording and answers the statements since [`start`].
pub fn take() -> Vec<(String, Duration)> {
    RECORDING
        .lock()
        .ok()
        .and_then(|mut recording| recording.take())
        .unwrap_or_default()
}

struct Statements;

impl<S: Subscriber> Layer<S> for Statements {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let Ok(mut recording) = RECORDING.lock() else {
            return;
        };
        let Some(statements) = recording.as_mut() else {
            return;
        };
        let mut fields = Fields::default();
        event.record(&mut fields);
        let text = if fields.statement.trim().is_empty() {
            fields.summary
        } else {
            fields.statement
        };
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        statements.push((text, Duration::from_secs_f64(fields.elapsed.max(0.0))));
    }
}

#[derive(Default)]
struct Fields {
    summary: String,
    statement: String,
    elapsed: f64,
}

impl Visit for Fields {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.elapsed = value;
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "summary" => value.clone_into(&mut self.summary),
            "db.statement" => value.clone_into(&mut self.statement),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "summary" => self.summary = format!("{value:?}").trim_matches('"').to_owned(),
            "db.statement" => {
                self.statement = format!("{value:?}").trim_matches('"').replace("\\n", " ");
            }
            _ => {}
        }
    }
}

/// One statement's share of a scenario.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub text: String,
    /// Runs per operation.
    pub calls: f64,
    /// Time per operation.
    pub time: Duration,
}

/// Statements of a scenario's operations, summed.
#[derive(Debug, Clone, Default)]
pub struct Totals {
    operations: usize,
    by_text: HashMap<String, (usize, Duration)>,
}

impl Totals {
    pub fn add(&mut self, statements: Vec<(String, Duration)>) {
        self.operations += 1;
        for (text, time) in statements {
            let entry = self.by_text.entry(text).or_default();
            entry.0 += 1;
            entry.1 += time;
        }
    }

    /// Statements per operation, the most time first.
    pub fn statements(&self) -> Vec<Statement> {
        let n = self.operations.max(1);
        let mut out: Vec<Statement> = self
            .by_text
            .iter()
            .map(|(text, (calls, time))| Statement {
                text: text.clone(),
                calls: *calls as f64 / n as f64,
                time: *time / u32::try_from(n).unwrap_or(u32::MAX),
            })
            .collect();
        out.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.text.cmp(&b.text)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_are_per_operation_and_sorted_by_time() {
        let mut totals = Totals::default();
        let ms = Duration::from_millis;
        totals.add(vec![("select a".into(), ms(2)), ("select b".into(), ms(1))]);
        totals.add(vec![("select a".into(), ms(4)), ("select a".into(), ms(2))]);
        let statements = totals.statements();
        assert_eq!(statements[0].text, "select a");
        assert!((statements[0].calls - 1.5).abs() < f64::EPSILON);
        assert_eq!(statements[0].time, ms(4));
        assert_eq!(statements[1].time, Duration::from_micros(500));
    }
}
