//! Personal interface settings (UI-018, D-041): each person reads and changes only their own.

use trackflow_domain::UserId;
use trackflow_domain::preferences::{self as rules, Patch, Preferences};
use trackflow_store::{PgPool, preferences};

use crate::{Error, Result};

/// The person's settings; unset keys are `None`.
pub async fn get(pool: &PgPool, user: UserId) -> Result<Preferences> {
    let stored = preferences::get(pool, user).await?;
    Ok(stored.map(rules::from_stored).unwrap_or_default())
}

/// Sets and resets keys of the person's settings; answers them all.
pub async fn update(pool: &PgPool, user: UserId, patch: Patch) -> Result<Preferences> {
    let valid = rules::validate(patch).map_err(Error::Invalid)?;
    let set = serde_json::to_value(&valid.set).map_err(|e| Error::Internal(e.to_string()))?;
    let reset: Vec<String> = valid.reset.iter().map(|k| (*k).to_owned()).collect();
    let mut tx = pool.begin().await?;
    let stored = preferences::patch(&mut tx, user, &set, &reset).await?;
    rules::check_size(&stored).map_err(|r| Error::rejected("preferences", r))?;
    tx.commit().await?;
    Ok(rules::from_stored(stored))
}
