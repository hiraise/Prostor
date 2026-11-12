//! Audit events of administrative changes (architecture.md §9.17). Capture starts here;
//! the viewer, filters and retention come with ADM-040…ADM-042.

use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use trackflow_domain::{Actor, UserId};

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub id: i64,
    pub at: OffsetDateTime,
    /// `None` when the server itself made the change.
    pub actor_id: Option<UserId>,
    pub action: String,
    pub target: String,
    pub details: Value,
}

/// Records an event as part of the caller's transaction.
pub async fn record(
    conn: &mut PgConnection,
    actor: Actor,
    action: &str,
    target: &str,
    details: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into audit_events (actor_id, action, target, details) values ($1, $2, $3, $4)",
        actor.user_id(),
        action,
        target,
        details,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Latest events about one target, newest first.
pub async fn for_target(
    pool: &PgPool,
    target: &str,
    limit: i64,
) -> Result<Vec<AuditEvent>, sqlx::Error> {
    sqlx::query_as!(
        AuditEvent,
        "select id, at, actor_id, action, target, details from audit_events
         where target = $1 order by id desc limit $2",
        target,
        limit,
    )
    .fetch_all(pool)
    .await
}
