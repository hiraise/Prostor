//! Custom fields of the instance (FLD-015…FLD-023) and their attachments to projects
//! (FLD-051…FLD-060). Rules: `trackflow_domain::fields`.

use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::{FieldId, ProjectId, SetId, UserId, ValueId};

/// Name space of the advisory locks taken here (`'fld'`).
const LOCK_SPACE: i32 = 0x0066_6c64;

/// Serializes finding or making the fields of project templates until the transaction ends, so
/// two projects made at once never both create the same template field.
pub async fn lock_templates(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query!("select pg_advisory_xact_lock($1, 0)", LOCK_SPACE)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub id: FieldId,
    pub name: String,
    /// `trackflow_domain::fields::FieldKind` key.
    pub kind: String,
    pub multi: bool,
    pub aliases: Vec<String>,
    /// Key of a field of the default set.
    pub builtin: Option<String>,
    pub auto_attach: bool,
    pub rank: String,
    pub set_id: Option<SetId>,
    pub can_be_empty: bool,
    pub empty_label: String,
    pub private: bool,
    /// Value IDs, or account IDs for user fields.
    pub default_values: Vec<i64>,
    pub version: i64,
    pub created_at: OffsetDateTime,
}

/// Unique constraint on (name, type); a violation names it.
pub const NAME_KEY: &str = "fields_name_key";

/// A field to create. Values are already validated.
#[derive(Debug, Clone)]
pub struct NewField<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub multi: bool,
    pub aliases: &'a [String],
    pub builtin: Option<&'a str>,
    pub auto_attach: bool,
    pub rank: &'a str,
    pub set_id: Option<SetId>,
    pub can_be_empty: bool,
    pub empty_label: &'a str,
    pub private: bool,
    pub default_values: &'a [i64],
}

pub async fn insert(conn: &mut PgConnection, field: &NewField<'_>) -> Result<FieldId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into fields (name, kind, multi, aliases, builtin, auto_attach, rank, set_id,
                             can_be_empty, empty_label, private, default_values)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) returning id",
        field.name,
        field.kind,
        field.multi,
        field.aliases,
        field.builtin,
        field.auto_attach,
        field.rank,
        field.set_id,
        field.can_be_empty,
        field.empty_label,
        field.private,
        field.default_values,
    )
    .fetch_one(conn)
    .await
}

pub async fn get(db: impl PgExecutor<'_>, id: FieldId) -> Result<Option<Field>, sqlx::Error> {
    sqlx::query_as!(
        Field,
        "select id, name, kind, multi, aliases, builtin, auto_attach, rank, set_id, can_be_empty,
                empty_label, private, default_values, version, created_at
         from fields where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(db: impl PgExecutor<'_>, ids: &[FieldId]) -> Result<Vec<Field>, sqlx::Error> {
    sqlx::query_as!(
        Field,
        "select id, name, kind, multi, aliases, builtin, auto_attach, rank, set_id, can_be_empty,
                empty_label, private, default_values, version, created_at
         from fields where id = any($1)",
        ids
    )
    .fetch_all(db)
    .await
}

pub async fn lock(conn: &mut PgConnection, id: FieldId) -> Result<Option<Field>, sqlx::Error> {
    sqlx::query_as!(
        Field,
        "select id, name, kind, multi, aliases, builtin, auto_attach, rank, set_id, can_be_empty,
                empty_label, private, default_values, version, created_at
         from fields where id = $1 for update",
        id
    )
    .fetch_optional(conn)
    .await
}

/// Every field in the instance order (FLD-022). An instance has tens of fields, rarely hundreds.
pub async fn all(db: impl PgExecutor<'_>) -> Result<Vec<Field>, sqlx::Error> {
    sqlx::query_as!(
        Field,
        "select id, name, kind, multi, aliases, builtin, auto_attach, rank, set_id, can_be_empty,
                empty_label, private, default_values, version, created_at
         from fields order by rank, id"
    )
    .fetch_all(db)
    .await
}

/// Fields attached to the projects a viewer reads (QRY-078, FLD-062), most used first, then in
/// the instance order, with how many of those projects use each. `read` and `private` are the
/// projects with issue reading and private-field reading, `None` for all; a private attachment
/// counts only where `private` holds. Projects being deleted are out; archived ones too unless
/// `within` names one.
pub async fn of_readable_projects(
    db: impl PgExecutor<'_>,
    read: Option<&[ProjectId]>,
    private: Option<&[ProjectId]>,
    within: Option<ProjectId>,
) -> Result<Vec<(Field, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select f.id, f.name, f.kind, f.multi, f.aliases, f.builtin, f.auto_attach, f.rank,
                  f.set_id, f.can_be_empty, f.empty_label, f.private, f.default_values, f.version,
                  f.created_at, count(*) as "uses!"
           from project_fields pf
           join projects p on p.id = pf.project_id
           join fields f on f.id = pf.field_id
           where p.deleting_at is null
             and ($3::bigint is null or p.id = $3)
             and ($3::bigint is not null or p.archived_at is null)
             and ($1::bigint[] is null or p.id = any($1))
             and (not pf.private or $2::bigint[] is null or p.id = any($2))
           group by f.id
           order by count(*) desc, f.rank, f.id"#,
        read,
        private,
        within,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                Field {
                    id: r.id,
                    name: r.name,
                    kind: r.kind,
                    multi: r.multi,
                    aliases: r.aliases,
                    builtin: r.builtin,
                    auto_attach: r.auto_attach,
                    rank: r.rank,
                    set_id: r.set_id,
                    can_be_empty: r.can_be_empty,
                    empty_label: r.empty_label,
                    private: r.private,
                    default_values: r.default_values,
                    version: r.version,
                    created_at: r.created_at,
                },
                r.uses,
            )
        })
        .collect())
}

/// Fields attached to every new blank project (FLD-020), in the instance order.
pub async fn auto_attached(db: impl PgExecutor<'_>) -> Result<Vec<Field>, sqlx::Error> {
    sqlx::query_as!(
        Field,
        "select id, name, kind, multi, aliases, builtin, auto_attach, rank, set_id, can_be_empty,
                empty_label, private, default_values, version, created_at
         from fields where auto_attach order by rank, id"
    )
    .fetch_all(db)
    .await
}

pub async fn by_builtin(db: impl PgExecutor<'_>, key: &str) -> Result<Option<Field>, sqlx::Error> {
    sqlx::query_as!(
        Field,
        "select id, name, kind, multi, aliases, builtin, auto_attach, rank, set_id, can_be_empty,
                empty_label, private, default_values, version, created_at
         from fields where builtin = $1",
        key
    )
    .fetch_optional(db)
    .await
}

/// The field of this name (ignoring case) and type (FLD-016).
pub async fn find(
    db: impl PgExecutor<'_>,
    name: &str,
    kind: &str,
    multi: bool,
) -> Result<Option<Field>, sqlx::Error> {
    sqlx::query_as!(
        Field,
        "select id, name, kind, multi, aliases, builtin, auto_attach, rank, set_id, can_be_empty,
                empty_label, private, default_values, version, created_at
         from fields where lower(name) = lower($1) and kind = $2 and multi = $3",
        name,
        kind,
        multi
    )
    .fetch_optional(db)
    .await
}

/// The last rank of the instance order.
pub async fn last_rank(db: impl PgExecutor<'_>) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(r#"select max(rank collate "C") as "rank" from fields"#)
        .fetch_one(db)
        .await
}

/// The first rank after `rank` in the instance order.
pub async fn rank_after(
    db: impl PgExecutor<'_>,
    rank: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select min(rank collate "C") as "rank" from fields where rank > $1::text collate "C""#,
        rank
    )
    .fetch_one(db)
    .await
}

/// The first rank of the instance order.
pub async fn first_rank(db: impl PgExecutor<'_>) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(r#"select min(rank collate "C") as "rank" from fields"#)
        .fetch_one(db)
        .await
}

/// Changes of a field's base properties and the settings of new attachments; `None` keeps one.
#[derive(Debug, Clone, Default)]
pub struct FieldChange<'a> {
    pub name: Option<&'a str>,
    pub aliases: Option<&'a [String]>,
    pub auto_attach: Option<bool>,
    pub can_be_empty: Option<bool>,
    pub empty_label: Option<&'a str>,
    pub private: Option<bool>,
    pub default_values: Option<&'a [i64]>,
    pub rank: Option<&'a str>,
}

pub async fn update(
    conn: &mut PgConnection,
    id: FieldId,
    change: &FieldChange<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update fields set
           name = coalesce($2, name),
           aliases = coalesce($3, aliases),
           auto_attach = coalesce($4, auto_attach),
           can_be_empty = coalesce($5, can_be_empty),
           empty_label = coalesce($6, empty_label),
           private = coalesce($7, private),
           default_values = coalesce($8, default_values),
           rank = coalesce($9, rank),
           version = version + 1,
           updated_at = now()
         where id = $1",
        id,
        change.name,
        change.aliases,
        change.auto_attach,
        change.can_be_empty,
        change.empty_label,
        change.private,
        change.default_values,
        change.rank,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut PgConnection, id: FieldId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from fields where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// How many projects use each field.
pub async fn project_counts(db: impl PgExecutor<'_>) -> Result<Vec<(FieldId, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select field_id, count(*) as "count!" from project_fields group by field_id"#
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.field_id, r.count)).collect())
}

/// The projects using a field.
pub async fn projects_of(
    db: impl PgExecutor<'_>,
    field: FieldId,
) -> Result<Vec<ProjectId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select project_id from project_fields where field_id = $1 order by project_id",
        field
    )
    .fetch_all(db)
    .await
}

/// Removes a deleted value from every list of default values.
pub async fn forget_default_value(
    conn: &mut PgConnection,
    value: ValueId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update fields set default_values = array_remove(default_values, $1), version = version + 1
         where $1 = any(default_values) and kind <> 'user'",
        value
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update project_fields p set default_values = array_remove(p.default_values, $1),
                                     version = p.version + 1
         from fields f
         where f.id = p.field_id and f.kind <> 'user' and $1 = any(p.default_values)",
        value
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Replaces deleted accounts in the default values of user fields (architecture.md §6); with
/// `None` they are removed.
pub async fn replace_default_users(
    conn: &mut PgConnection,
    users: &[UserId],
    replacement: Option<UserId>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update fields set
           default_values = array(select distinct coalesce(case when v = any($1) then $2::bigint end, v)
                                  from unnest(default_values) v
                                  where not (v = any($1) and $2::bigint is null)),
           version = version + 1
         where kind = 'user' and default_values && $1",
        users,
        replacement
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update project_fields p set
           default_values = array(select distinct coalesce(case when v = any($1) then $2::bigint end, v)
                                  from unnest(p.default_values) v
                                  where not (v = any($1) and $2::bigint is null)),
           version = p.version + 1
         from fields f
         where f.id = p.field_id and f.kind = 'user' and p.default_values && $1",
        users,
        replacement
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A field attached to a project with its settings there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    pub project_id: ProjectId,
    pub field_id: FieldId,
    pub set_id: Option<SetId>,
    pub rank: String,
    pub can_be_empty: bool,
    pub empty_label: String,
    pub default_values: Vec<i64>,
    pub private: bool,
    pub version: i64,
    pub created_at: OffsetDateTime,
}

/// An attachment to create.
#[derive(Debug, Clone)]
pub struct Attachment<'a> {
    pub project_id: ProjectId,
    pub field_id: FieldId,
    pub set_id: Option<SetId>,
    pub rank: &'a str,
    pub can_be_empty: bool,
    pub empty_label: &'a str,
    pub default_values: &'a [i64],
    pub private: bool,
}

pub async fn attach(
    conn: &mut PgConnection,
    attachment: &Attachment<'_>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "insert into project_fields (project_id, field_id, set_id, rank, can_be_empty, empty_label,
                                     default_values, private)
         values ($1, $2, $3, $4, $5, $6, $7, $8) on conflict do nothing",
        attachment.project_id,
        attachment.field_id,
        attachment.set_id,
        attachment.rank,
        attachment.can_be_empty,
        attachment.empty_label,
        attachment.default_values,
        attachment.private,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// The fields of a project in its order (FLD-058).
pub async fn of_project(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<Vec<Attached>, sqlx::Error> {
    sqlx::query_as!(
        Attached,
        "select project_id, field_id, set_id, rank, can_be_empty, empty_label, default_values,
                private, version, created_at
         from project_fields where project_id = $1 order by rank, field_id",
        project
    )
    .fetch_all(db)
    .await
}

/// The fields of several projects, each in its order.
pub async fn of_projects(
    db: impl PgExecutor<'_>,
    projects: &[ProjectId],
) -> Result<Vec<Attached>, sqlx::Error> {
    sqlx::query_as!(
        Attached,
        "select project_id, field_id, set_id, rank, can_be_empty, empty_label, default_values,
                private, version, created_at
         from project_fields where project_id = any($1) order by project_id, rank, field_id",
        projects
    )
    .fetch_all(db)
    .await
}

/// The attachments of a field in every project.
pub async fn attachments_of(
    db: impl PgExecutor<'_>,
    field: FieldId,
) -> Result<Vec<Attached>, sqlx::Error> {
    sqlx::query_as!(
        Attached,
        "select project_id, field_id, set_id, rank, can_be_empty, empty_label, default_values,
                private, version, created_at
         from project_fields where field_id = $1 order by project_id",
        field
    )
    .fetch_all(db)
    .await
}

pub async fn lock_attached(
    conn: &mut PgConnection,
    project: ProjectId,
    field: FieldId,
) -> Result<Option<Attached>, sqlx::Error> {
    sqlx::query_as!(
        Attached,
        "select project_id, field_id, set_id, rank, can_be_empty, empty_label, default_values,
                private, version, created_at
         from project_fields where project_id = $1 and field_id = $2 for update",
        project,
        field
    )
    .fetch_optional(conn)
    .await
}

/// The last rank of a project's fields.
pub async fn last_attached_rank(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select max(rank collate "C") as "rank" from project_fields where project_id = $1"#,
        project
    )
    .fetch_one(db)
    .await
}

/// The first rank after `rank` among a project's fields.
pub async fn attached_rank_after(
    db: impl PgExecutor<'_>,
    project: ProjectId,
    rank: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select min(rank collate "C") as "rank" from project_fields
           where project_id = $1 and rank > $2::text collate "C""#,
        project,
        rank
    )
    .fetch_one(db)
    .await
}

/// The first rank of a project's fields.
pub async fn first_attached_rank(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select min(rank collate "C") as "rank" from project_fields where project_id = $1"#,
        project
    )
    .fetch_one(db)
    .await
}

/// Changes of an attachment; `None` keeps a setting.
#[derive(Debug, Clone, Default)]
pub struct AttachedChange<'a> {
    pub set_id: Option<SetId>,
    pub rank: Option<&'a str>,
    pub can_be_empty: Option<bool>,
    pub empty_label: Option<&'a str>,
    pub default_values: Option<&'a [i64]>,
    pub private: Option<bool>,
}

pub async fn update_attached(
    conn: &mut PgConnection,
    project: ProjectId,
    field: FieldId,
    change: &AttachedChange<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update project_fields set
           set_id = coalesce($3, set_id),
           rank = coalesce($4, rank),
           can_be_empty = coalesce($5, can_be_empty),
           empty_label = coalesce($6, empty_label),
           default_values = coalesce($7, default_values),
           private = coalesce($8, private),
           version = version + 1,
           updated_at = now()
         where project_id = $1 and field_id = $2",
        project,
        field,
        change.set_id,
        change.rank,
        change.can_be_empty,
        change.empty_label,
        change.default_values,
        change.private,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn detach(
    conn: &mut PgConnection,
    project: ProjectId,
    field: FieldId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "delete from project_fields where project_id = $1 and field_id = $2",
        project,
        field
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// The sets a project uses, for cleaning up after it is deleted.
pub async fn sets_of_project(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<Vec<SetId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select distinct set_id as "id!" from project_fields
           where project_id = $1 and set_id is not null"#,
        project
    )
    .fetch_all(db)
    .await
}

/// Removes accounts from the default values of user fields that take values from a set: they are
/// no longer offered there.
pub async fn forget_default_users_of_set(
    conn: &mut PgConnection,
    set: SetId,
    users: &[UserId],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update fields set default_values = array(select v from unnest(default_values) v
                                                  where not v = any($2)),
                           version = version + 1
         where set_id = $1 and kind = 'user' and default_values && $2",
        set,
        users
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update project_fields p set
           default_values = array(select v from unnest(p.default_values) v where not v = any($2)),
           version = p.version + 1
         from fields f
         where f.id = p.field_id and f.kind = 'user' and p.set_id = $1
           and p.default_values && $2",
        set,
        users
    )
    .execute(conn)
    .await?;
    Ok(())
}
