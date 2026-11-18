//! Personal interface settings (UI-018, D-041): one JSON object per account. Keys and their rules:
//! `prostor_domain::preferences`.

use prostor_domain::UserId;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};

/// What a person has set; `None` before their first change.
pub async fn get(db: impl PgExecutor<'_>, user: UserId) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!("select data from user_preferences where user_id = $1", user)
        .fetch_optional(db)
        .await
}

/// Merges `set` into a person's settings and drops the keys of `reset`, in one statement; answers
/// the object as stored. Last write wins per key.
pub async fn patch(
    conn: &mut PgConnection,
    user: UserId,
    set: &Value,
    reset: &[String],
) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into user_preferences (user_id, data) values ($1, $2::jsonb - $3::text[])
         on conflict (user_id) do update
           set data = (user_preferences.data || $2::jsonb) - $3::text[], updated_at = now()
         returning data",
        user,
        set,
        reset
    )
    .fetch_one(conn)
    .await
}
