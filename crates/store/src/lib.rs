//! PostgreSQL storage (D-004): SQL only (D-021). Connection pool, migrations, the notification bus,
//! settings, audit events, the job queue, accounts, sessions, tokens, groups, roles, role
//! assignments, one-time links, projects and their teams, fields and value sets, issues with their
//! values, history, tags, watchers and votes, comments and rendered texts, staged uploads and
//! attachments, the live event log, the per-project rule toggles. Queries use sqlx
//! compile-checked macros (conventions.md §2).

pub mod access;
pub mod account_links;
pub mod api_tokens;
pub mod attachments;
pub mod audit;
pub mod boards;
pub mod bus;
pub mod command_history;
pub mod comments;
pub mod content;
pub mod db;
pub mod fields;
pub mod groups;
pub mod instance;
pub mod issue_history;
pub mod issue_values;
pub mod issues;
pub mod jobs;
pub mod links;
pub mod live;
pub mod notifications;
pub mod preferences;
pub mod projects;
pub mod roles;
pub mod rules;
pub mod saved_searches;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod tags;
pub mod teams;
pub mod uploads;
pub mod users;
pub mod value_sets;
pub mod visibility;

pub use db::{MIGRATOR, MigrationReport, connect_options, migrate, ping, pool};
pub use sqlx::postgres::PgConnectOptions;
pub use sqlx::{PgConnection, PgPool, Postgres, Transaction};

/// The name of the unique constraint or index a failed write violated, if that was the failure.
pub fn unique_violation(error: &sqlx::Error) -> Option<&str> {
    match error {
        sqlx::Error::Database(db) if db.is_unique_violation() => db.constraint(),
        _ => None,
    }
}
