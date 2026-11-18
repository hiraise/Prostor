//! The command history (step 2.1 §6, CMD-030): the command lines a person applied, at most
//! [`KEEP`], feeding the completion's `recent` group and the dialog's «repeat the last command».

use prostor_domain::UserId;
use sqlx::{Acquire, PgConnection, PgExecutor};

/// How many lines one person's history keeps; the rest is trimmed on every record.
pub const KEEP: usize = 20;

/// Records a line a person applied: an upsert that bumps `used_at`, then the trim back to
/// [`KEEP`], in one transaction.
pub async fn record(
    conn: &mut PgConnection,
    user: UserId,
    command: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = conn.begin().await?;
    sqlx::query!(
        "insert into command_history (user_id, command, used_at)
         values ($1, $2, now())
         on conflict (user_id, command) do update set used_at = now()",
        user,
        command,
    )
    .execute(&mut *tx)
    .await?;
    // `command` breaks `used_at` ties, so the trim stays deterministic (plan §0).
    sqlx::query!(
        "delete from command_history
         where user_id = $1 and command in (
           select command from command_history
           where user_id = $1
           order by used_at desc, command
           offset $2
         )",
        user,
        KEEP as i64,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// The person's lines, newest first, at most `limit`.
pub async fn recent(
    db: impl PgExecutor<'_>,
    user: UserId,
    limit: i64,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "select command from command_history
         where user_id = $1
         order by used_at desc, command
         limit $2",
        user,
        limit,
    )
    .fetch_all(db)
    .await
}
