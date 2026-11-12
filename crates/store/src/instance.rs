//! Facts about this installation: the state of the first-run setup (ADM-047).

use sqlx::{PgConnection, PgPool};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupState {
    /// Nobody has run the setup yet; the wizard asks for this code.
    Pending {
        code: String,
    },
    Complete,
}

/// The setup state, making sure a pending setup has a code. `configured` (from the server's
/// environment) replaces the stored code; otherwise a stored code is kept, so restarts and other
/// nodes print the same one, and `generated` is used only for the very first one.
pub async fn prepare_setup(
    pool: &PgPool,
    generated: &str,
    configured: Option<&str>,
) -> Result<SetupState, sqlx::Error> {
    let row = sqlx::query!(
        "insert into instance (id, setup_code) values (true, coalesce($2, $1))
         on conflict (id) do update
         set setup_code = case
               when instance.setup_completed_at is not null then null
               else coalesce($2, instance.setup_code, $1)
             end
         returning setup_code, setup_completed_at",
        generated,
        configured,
    )
    .fetch_one(pool)
    .await?;
    Ok(match (row.setup_completed_at, row.setup_code) {
        (None, Some(code)) => SetupState::Pending { code },
        _ => SetupState::Complete,
    })
}

/// Whether the setup is complete.
pub async fn setup_complete(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let complete = sqlx::query_scalar!(
        r#"select exists(select 1 from instance where setup_completed_at is not null) as "complete!""#
    )
    .fetch_one(pool)
    .await?;
    Ok(complete)
}

/// The pending setup's code, locked so two setups cannot run at once; `None` when the setup is
/// complete (or no node has prepared it).
pub async fn lock_pending_setup(conn: &mut PgConnection) -> Result<Option<String>, sqlx::Error> {
    let code = sqlx::query_scalar!(
        "select setup_code from instance where setup_completed_at is null for update"
    )
    .fetch_optional(conn)
    .await?;
    Ok(code.flatten())
}

/// Marks the setup complete and forgets its code.
pub async fn complete_setup(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query!("update instance set setup_completed_at = now(), setup_code = null")
        .execute(conn)
        .await?;
    Ok(())
}

/// The instance's secret key (pseudonyms, ACC-016), storing `generated` when there is none yet.
/// Every node gets the same key.
pub async fn secret(pool: &PgPool, generated: &[u8]) -> Result<Vec<u8>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"update instance set secret = coalesce(secret, $1) returning secret as "secret!""#,
        generated,
    )
    .fetch_one(pool)
    .await
}

/// Locks the instance row when the setup is complete and the default field set has not been
/// created yet (FLD-070); `false` otherwise. The caller creates it in the same transaction and
/// calls [`fields_seeded`], so only one node ever does.
pub async fn lock_unseeded_fields(conn: &mut PgConnection) -> Result<bool, sqlx::Error> {
    let found = sqlx::query_scalar!(
        "select true as \"found!\" from instance
         where setup_completed_at is not null and fields_seeded_at is null for update"
    )
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

/// Locks the instance row when the setup is complete and the built-in link types (LNK-002) were
/// not created yet; `true` then.
pub async fn lock_unseeded_link_types(conn: &mut PgConnection) -> Result<bool, sqlx::Error> {
    let found = sqlx::query_scalar!(
        "select true as \"found!\" from instance
         where setup_completed_at is not null and link_types_seeded_at is null for update"
    )
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

/// Records that the built-in link types exist.
pub async fn link_types_seeded(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query!("update instance set link_types_seeded_at = now()")
        .execute(conn)
        .await?;
    Ok(())
}

/// Records that the default field set exists.
pub async fn fields_seeded(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query!("update instance set fields_seeded_at = now()")
        .execute(conn)
        .await?;
    Ok(())
}
