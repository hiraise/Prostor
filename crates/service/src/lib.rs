//! Use cases (D-021): one async function per change or read, owning its transaction. A use case
//! loads what the domain rule needs, calls the rule, writes through `store`, records audit events
//! and notifications, and commits. HTTP stays in the server; SQL stays in the store.

pub mod access;
pub mod assist;
pub mod attachments;

pub mod account;
pub mod auth;
pub mod boards;
pub mod capabilities;
pub mod commands;
pub mod comments;
mod error;
pub mod fields;
pub mod files;
pub mod groups;
pub mod images;
pub mod issue_links;
pub mod issues;
pub mod jobs;
pub mod links;
pub mod live;
pub mod mail;
pub mod notifications;
pub mod passwords;
pub mod people;
pub mod pictures;
pub mod preferences;
pub mod project_fields;
pub mod projects;
pub mod recovery;
pub mod roles;
pub mod rules;
pub mod saved_searches;
pub mod search;
pub mod secrets;
pub mod seed;
pub mod settings;
pub mod setup;
pub mod tags;
pub mod teams;
pub mod text;
pub mod tokens;
pub mod uploads;
pub mod users;
pub mod value_sets;

pub use error::{Error, Result};
pub use passwords::Passwords;
