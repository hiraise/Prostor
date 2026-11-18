//! Stored setting values (ADM-002). Meaning and validation: `prostor_domain::settings`; the
//! change flow (audit, notification): `prostor_service::settings`. Secret values (NTF-004)
//! live as ciphertext in their own table, sealed with the instance key.

use prostor_domain::Actor;
use prostor_domain::secret::Key;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};

/// All stored `(key, value)` pairs; settings without a row have their default. Secret settings
/// come from [`load_secrets`] instead.
pub async fn load(pool: &PgPool) -> Result<Vec<(String, Value)>, sqlx::Error> {
    let rows = sqlx::query!("select key, value from settings")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|row| (row.key, row.value)).collect())
}

/// The decrypted `(key, value)` pairs of secret settings. Rows that do not open under this key
/// are skipped and named, so a rotated-away key leaves the defaults in place and never stops the
/// node. Without a key, every secret sits at its default.
pub async fn load_secrets(
    pool: &PgPool,
    key: Option<&Key>,
) -> Result<(Vec<(String, Value)>, Vec<String>), sqlx::Error> {
    let rows = sqlx::query!("select key, value from setting_secrets")
        .fetch_all(pool)
        .await?;
    let Some(key) = key else {
        return Ok((Vec::new(), rows.into_iter().map(|row| row.key).collect()));
    };
    let mut pairs = Vec::new();
    let mut unreadable = Vec::new();
    for row in rows {
        match key.open(&row.key, &row.value).map(Value::String) {
            Some(value) => pairs.push((row.key, value)),
            None => unreadable.push(row.key),
        }
    }
    Ok((pairs, unreadable))
}

/// The secret value of a setting as plain JSON; `None` means the default.
pub async fn secret_value(
    conn: &mut PgConnection,
    key: &Key,
    setting: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let sealed = sqlx::query_scalar!("select value from setting_secrets where key = $1", setting)
        .fetch_optional(conn)
        .await?;
    Ok(sealed
        .as_deref()
        .and_then(|sealed| key.open(setting, sealed))
        .map(Value::String))
}

/// Whether a secret setting has a stored value, without opening it.
pub async fn secret_exists(conn: &mut PgConnection, setting: &str) -> Result<bool, sqlx::Error> {
    // `exists` answers NULL only for no rows, which `fetch_one` on `exists` never produces.
    Ok(sqlx::query_scalar!(
        "select exists(select 1 from setting_secrets where key = $1)",
        setting
    )
    .fetch_one(conn)
    .await?
    .unwrap_or(false))
}

/// Stores an already-sealed value (the caller checks that sealing worked).
pub async fn secret_upsert(
    conn: &mut PgConnection,
    setting: &str,
    sealed: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into setting_secrets (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value",
        setting,
        sealed,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns a secret setting to its default.
pub async fn secret_remove(conn: &mut PgConnection, setting: &str) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from setting_secrets where key = $1", setting)
        .execute(conn)
        .await?;
    Ok(())
}

/// The stored value of a setting; `None` means the default.
pub async fn value(conn: &mut PgConnection, key: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!("select value from settings where key = $1", key)
        .fetch_optional(conn)
        .await
}

/// The stored value of a setting, locked until the transaction ends; `None` means the default.
pub async fn lock_value(conn: &mut PgConnection, key: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!("select value from settings where key = $1 for update", key)
        .fetch_optional(conn)
        .await
}

/// Stores a value that differs from the default.
pub async fn upsert(
    conn: &mut PgConnection,
    key: &str,
    value: &Value,
    actor: Actor,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into settings (key, value, updated_by) values ($1, $2, $3)
         on conflict (key) do update
         set value = excluded.value, updated_at = now(), updated_by = excluded.updated_by",
        key,
        value,
        actor.user_id(),
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns a setting to its default.
pub async fn remove(conn: &mut PgConnection, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from settings where key = $1", key)
        .execute(conn)
        .await?;
    Ok(())
}
