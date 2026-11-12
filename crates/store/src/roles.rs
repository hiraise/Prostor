//! Roles (ACC-052…ACC-057). Predefined roles have rows (assignments point at them) but take their
//! permissions from `trackflow_domain::access::role`.

use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::RoleId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: RoleId,
    /// Key of a predefined role; `None` for custom roles.
    pub builtin: Option<String>,
    pub name: String,
    pub description: String,
    /// Permission keys of a custom role; empty for predefined ones.
    pub permissions: Vec<String>,
    pub version: i64,
    pub created_at: OffsetDateTime,
}

/// Unique constraint on names; a violation names it.
pub const NAME_KEY: &str = "roles_name_key";

/// The most roles an instance is expected to have; lists load all of them at once.
pub const MAX_ROLES: i64 = 1000;

pub async fn insert(
    conn: &mut PgConnection,
    name: &str,
    description: &str,
    permissions: &[String],
) -> Result<RoleId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into roles (name, description, permissions) values ($1, $2, $3) returning id",
        name,
        description,
        permissions,
    )
    .fetch_one(conn)
    .await
}

pub async fn get(db: impl PgExecutor<'_>, id: RoleId) -> Result<Option<Role>, sqlx::Error> {
    sqlx::query_as!(
        Role,
        "select id, builtin, name, description, permissions, version, created_at
         from roles where id = $1",
        id,
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(db: impl PgExecutor<'_>, ids: &[RoleId]) -> Result<Vec<Role>, sqlx::Error> {
    sqlx::query_as!(
        Role,
        "select id, builtin, name, description, permissions, version, created_at
         from roles where id = any($1)",
        ids,
    )
    .fetch_all(db)
    .await
}

pub async fn lock(conn: &mut PgConnection, id: RoleId) -> Result<Option<Role>, sqlx::Error> {
    sqlx::query_as!(
        Role,
        "select id, builtin, name, description, permissions, version, created_at
         from roles where id = $1 for update",
        id,
    )
    .fetch_optional(conn)
    .await
}

/// The ID of a predefined role.
pub async fn builtin_id(db: impl PgExecutor<'_>, key: &str) -> Result<Option<RoleId>, sqlx::Error> {
    sqlx::query_scalar!("select id from roles where builtin = $1", key)
        .fetch_optional(db)
        .await
}

/// Every role, predefined ones first ([`MAX_ROLES`] at most).
pub async fn all(db: impl PgExecutor<'_>) -> Result<Vec<Role>, sqlx::Error> {
    sqlx::query_as!(
        Role,
        "select id, builtin, name, description, permissions, version, created_at
         from roles order by builtin is null, lower(name), id limit $1",
        MAX_ROLES,
    )
    .fetch_all(db)
    .await
}

/// Changes name and description; `None` keeps a value.
pub async fn update(
    conn: &mut PgConnection,
    id: RoleId,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update roles set name = coalesce($2, name), description = coalesce($3, description),
                          version = version + 1, updated_at = now()
         where id = $1",
        id,
        name,
        description,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Stores the permissions of a custom role.
pub async fn set_permissions(
    conn: &mut PgConnection,
    id: RoleId,
    permissions: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update roles set permissions = $2, version = version + 1, updated_at = now()
         where id = $1 and builtin is null",
        id,
        permissions,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes a custom role; its assignments go with it.
pub async fn delete(conn: &mut PgConnection, id: RoleId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from roles where id = $1 and builtin is null", id)
        .execute(conn)
        .await?;
    Ok(())
}
