//! The benchmark of the performance budget (architecture.md §7): a dataset generator, load
//! scenarios and a report. `make bench` runs it on PostgreSQL limited to the small server
//! (`deploy/compose.bench.yaml`), `make bench-stress` on 1M issues (`deploy/compose.stress.yaml`).
//!
//! Queries are built at run time (compiled lists, the prototype tables) and checked when they run,
//! not when they compile (D-015). The test of this crate runs every query on a small dataset, so
//! `make check` catches a broken one.

pub mod breakdown;
pub mod catalog;
pub mod dataset;
pub mod http;
pub mod report;
pub mod rng;
pub mod runner;
pub mod scenarios;
pub mod text;
