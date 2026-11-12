//! The API contract: request and response types of `/api/v1`.
//!
//! The OpenAPI document and the web app's TypeScript types are generated from these types (D-006).
//! HTTP handling (status codes, extractors) lives in the server; this crate has no web framework.
//! Conventions for every endpoint: `docs/architecture.md` → API conventions.

pub mod access;
pub mod accounts;
pub mod admin;
pub mod auth;
pub mod boards;
pub mod bootstrap;
pub mod comments;
pub mod content;
pub mod error;
pub mod fields;
pub mod files;
pub mod health;
pub mod issues;
pub mod links;
pub mod live;
pub mod notifications;
pub mod page;
pub mod patch;
pub mod preferences;
pub mod projects;
pub mod rules;
pub mod saved_searches;
pub mod search;
pub mod setup;
pub mod sharing;
