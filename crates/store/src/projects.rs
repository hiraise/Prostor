//! Projects, their keys and favorites (PRJ). Rules for the values: `trackflow_domain::projects`.

use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::{GroupId, ProjectId, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub key: String,
    pub name: String,
    pub description: String,
    /// The description rendered in the basic profile (PRJ-006).
    pub description_html: String,
    /// `standard` or `service_desk`.
    pub kind: String,
    /// Key of the built-in template it came from.
    pub template: String,
    pub owner_id: UserId,
    pub logo_color: i16,
    /// The uploaded logo in the file store (PRJ-007); `None` shows the generated one.
    pub logo_key: Option<String>,
    pub next_number: i64,
    pub default_audience_group_id: Option<GroupId>,
    pub archived_at: Option<OffsetDateTime>,
    pub deleting_at: Option<OffsetDateTime>,
    pub deletion_job_id: Option<i64>,
    pub created_by: Option<UserId>,
    pub version: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Project {
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    pub fn is_deleting(&self) -> bool {
        self.deleting_at.is_some()
    }
}

/// Unique constraints; a violation names one of these.
pub const KEY_KEY: &str = "project_keys_pkey";
pub const CURRENT_KEY_KEY: &str = "projects_key_key";
pub const NAME_KEY: &str = "projects_name_key";

/// A project to create. Values are already validated.
#[derive(Debug, Clone)]
pub struct NewProject<'a> {
    pub key: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub description_html: &'a str,
    pub render_version: i16,
    pub template: &'a str,
    pub owner: UserId,
    pub next_number: i64,
    pub created_by: Option<UserId>,
}

/// Creates a project and reserves its key. A key that some project has or had breaks
/// [`KEY_KEY`] or the unique key of `projects`.
pub async fn insert(
    conn: &mut PgConnection,
    project: &NewProject<'_>,
) -> Result<ProjectId, sqlx::Error> {
    let id = sqlx::query_scalar!(
        "insert into projects (key, name, description, template, owner_id, next_number, created_by,
                               description_html, render_version)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning id",
        project.key,
        project.name,
        project.description,
        project.template,
        project.owner,
        project.next_number,
        project.created_by,
        project.description_html,
        project.render_version,
    )
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query!(
        "insert into project_keys (key, project_id) values ($1, $2)",
        project.key,
        id
    )
    .execute(conn)
    .await?;
    Ok(id)
}

pub async fn get(db: impl PgExecutor<'_>, id: ProjectId) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        "select id, key, name, description, description_html, kind, template, owner_id, logo_color, logo_key, next_number,
                default_audience_group_id, archived_at, deleting_at, deletion_job_id, created_by, version,
                created_at, updated_at
         from projects where id = $1",
        id,
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(
    db: impl PgExecutor<'_>,
    ids: &[ProjectId],
) -> Result<Vec<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        "select id, key, name, description, description_html, kind, template, owner_id, logo_color, logo_key, next_number,
                default_audience_group_id, archived_at, deleting_at, deletion_job_id, created_by, version,
                created_at, updated_at
         from projects where id = any($1)",
        ids,
    )
    .fetch_all(db)
    .await
}

/// Every project, for small instance-wide lists and checks.
pub async fn all_ids(db: impl PgExecutor<'_>) -> Result<Vec<ProjectId>, sqlx::Error> {
    sqlx::query_scalar!("select id from projects order by id")
        .fetch_all(db)
        .await
}

pub async fn lock(conn: &mut PgConnection, id: ProjectId) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        "select id, key, name, description, description_html, kind, template, owner_id, logo_color, logo_key, next_number,
                default_audience_group_id, archived_at, deleting_at, deletion_job_id, created_by, version,
                created_at, updated_at
         from projects where id = $1 for update",
        id,
    )
    .fetch_optional(conn)
    .await
}

/// Locks projects in ID order, so concurrent changes of several never deadlock.
pub async fn lock_many(
    conn: &mut PgConnection,
    ids: &[ProjectId],
) -> Result<Vec<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        "select id, key, name, description, description_html, kind, template, owner_id, logo_color, logo_key, next_number,
                default_audience_group_id, archived_at, deleting_at, deletion_job_id, created_by, version,
                created_at, updated_at
         from projects where id = any($1) order by id for share",
        ids,
    )
    .fetch_all(conn)
    .await
}

/// The project with this key, current or former (PRJ-005). Keys are stored in capitals.
pub async fn by_key(db: impl PgExecutor<'_>, key: &str) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        "select p.id, p.key, p.name, p.description, p.description_html, p.kind, p.template, p.owner_id, p.logo_color, p.logo_key,
                p.next_number, p.default_audience_group_id, p.archived_at, p.deleting_at,
                p.deletion_job_id, p.created_by, p.version, p.created_at, p.updated_at
         from project_keys k join projects p on p.id = k.project_id
         where k.key = upper($1)",
        key,
    )
    .fetch_optional(db)
    .await
}

/// Which of these keys some project has or had.
pub async fn keys_taken(
    db: impl PgExecutor<'_>,
    keys: &[String],
) -> Result<Vec<(String, ProjectId)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select key, project_id from project_keys where key = any($1)",
        keys
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.key, r.project_id)).collect())
}

/// Keys starting with a prefix, for suggesting a free one.
pub async fn keys_starting_with(
    db: impl PgExecutor<'_>,
    prefix: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "select key from project_keys where key like $1 || '%' order by key limit 10000",
        prefix.replace(['%', '_', '\\'], ""),
    )
    .fetch_all(db)
    .await
}

/// Former keys of projects, oldest first: `(project, key)` pairs (PRJ-005).
pub async fn former_keys(
    db: impl PgExecutor<'_>,
    ids: &[ProjectId],
) -> Result<Vec<(ProjectId, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select k.project_id, k.key from project_keys k join projects p on p.id = k.project_id
         where k.project_id = any($1) and k.key <> p.key
         order by k.created_at, k.key",
        ids
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.project_id, r.key)).collect())
}

/// The project that has or had a key.
pub async fn key_owner(
    conn: &mut PgConnection,
    key: &str,
) -> Result<Option<ProjectId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select project_id from project_keys where key = $1 for update",
        key
    )
    .fetch_optional(conn)
    .await
}

/// Gives a project a new current key; the old one stays reserved for it (PRJ-005).
pub async fn set_key(conn: &mut PgConnection, id: ProjectId, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into project_keys (key, project_id) values ($1, $2) on conflict (key) do nothing",
        key,
        id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update projects set key = $2, version = version + 1, updated_at = now() where id = $1",
        id,
        key
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A description with its rendering.
#[derive(Debug, Clone, Copy)]
pub struct Description<'a> {
    pub text: &'a str,
    pub html: &'a str,
    pub render_version: i16,
}

/// Changes name and description; `None` keeps a value.
pub async fn update(
    conn: &mut PgConnection,
    id: ProjectId,
    name: Option<&str>,
    description: Option<Description<'_>>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update projects set name = coalesce($2, name), description = coalesce($3, description),
                             description_html = coalesce($4, description_html),
                             render_version = coalesce($5, render_version),
                             version = version + 1, updated_at = now()
         where id = $1",
        id,
        name,
        description.map(|d| d.text),
        description.map(|d| d.html),
        description.map(|d| d.render_version),
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_audience(
    conn: &mut PgConnection,
    id: ProjectId,
    group: Option<GroupId>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update projects set default_audience_group_id = $2, version = version + 1,
                             updated_at = now()
         where id = $1",
        id,
        group,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_owner(
    conn: &mut PgConnection,
    id: ProjectId,
    owner: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update projects set owner_id = $2, version = version + 1, updated_at = now()
         where id = $1",
        id,
        owner,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Archives (`true`) or restores (`false`) a project (PRJ-061, PRJ-063).
pub async fn set_archived(
    conn: &mut PgConnection,
    id: ProjectId,
    archived: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update projects set archived_at = case when $2 then now() end, version = version + 1,
                             updated_at = now()
         where id = $1",
        id,
        archived,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Marks a project as being deleted by a job (PRJ-065).
pub async fn mark_deleting(
    conn: &mut PgConnection,
    id: ProjectId,
    job: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update projects set deleting_at = coalesce(deleting_at, now()), deletion_job_id = $2,
                             version = version + 1, updated_at = now()
         where id = $1",
        id,
        job,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes the project row; its keys, team, favorites, assignments and field attachments go with
/// it. Its content must be gone already (the deletion job removes it in batches).
pub async fn delete(conn: &mut PgConnection, id: ProjectId) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!("delete from projects where id = $1", id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Moves projects owned by deleted accounts to their replacement (architecture.md §6).
pub async fn replace_owner(
    conn: &mut PgConnection,
    users: &[UserId],
    replacement: UserId,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "update projects set owner_id = $2, version = version + 1, updated_at = now()
         where owner_id = any($1)",
        users,
        replacement,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// Moves default audiences of deleted groups to their replacement (ACC-032).
pub async fn replace_audience(
    conn: &mut PgConnection,
    groups: &[GroupId],
    replacement: GroupId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update projects set default_audience_group_id = $2, version = version + 1,
                             updated_at = now()
         where default_audience_group_id = any($1)",
        groups,
        replacement,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Bumps the version after a change stored elsewhere (team, fields).
pub async fn touch(conn: &mut PgConnection, ids: &[ProjectId]) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update projects set version = version + 1, updated_at = now() where id = any($1)",
        ids
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Which part of the project list (PRJ-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Active,
    Archived,
    /// Being deleted (PRJ-065).
    Deleting,
}

/// A project of a list with the viewer's star.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub project: Project,
    pub favorite: bool,
}

/// What a page of the list continues after: `(favorite rank, lowercased name, id)`.
pub type ListCursor = (i32, String, ProjectId);

/// A page of projects by name, the viewer's favorites first in the active section (PRJ-012);
/// `visible` limits the page to these projects (`None`: every project). `text` matches a part of
/// the name or the start of the key.
pub async fn list(
    conn: &mut PgConnection,
    viewer: UserId,
    visible: Option<&[ProjectId]>,
    section: Section,
    text: &str,
    after: &ListCursor,
    limit: i64,
) -> Result<Vec<Listed>, sqlx::Error> {
    let (archived, deleting) = match section {
        Section::Active => (false, false),
        Section::Archived => (true, false),
        Section::Deleting => (false, true),
    };
    let stars = section == Section::Active;
    let rows = match visible {
        None => {
            sqlx::query!(
                r#"select p.id, p.key, p.name, p.description, p.description_html, p.kind, p.template, p.owner_id,
                          p.logo_color, p.logo_key, p.next_number, p.default_audience_group_id, p.archived_at,
                          p.deleting_at, p.deletion_job_id, p.created_by, p.version, p.created_at,
                          p.updated_at,
                          f.user_id is not null as "favorite!",
                          case when f.user_id is not null and $3 then 0 else 1 end as "rank!"
                   from projects p
                   left join project_favorites f on f.project_id = p.id and f.user_id = $1
                   where (p.archived_at is not null) = $4 and (p.deleting_at is not null) = $5
                     and (strpos(lower(p.name), lower($2)) > 0 or starts_with(p.key, upper($2)))
                     and (case when f.user_id is not null and $3 then 0 else 1 end,
                          lower(p.name), p.id) > ($6, lower($7), $8)
                   order by case when f.user_id is not null and $3 then 0 else 1 end, lower(p.name), p.id
                   limit $9"#,
                viewer,
                text,
                stars,
                archived,
                deleting,
                after.0,
                after.1,
                after.2,
                limit,
            )
            .fetch_all(conn)
            .await?
            .into_iter()
            .map(|r| Listed {
                project: Project {
                    id: r.id,
                    key: r.key,
                    name: r.name,
                    description: r.description,
                    description_html: r.description_html,
                    kind: r.kind,
                    template: r.template,
                    owner_id: r.owner_id,
                    logo_color: r.logo_color,
                    logo_key: r.logo_key,
                    next_number: r.next_number,
                    default_audience_group_id: r.default_audience_group_id,
                    archived_at: r.archived_at,
                    deleting_at: r.deleting_at,
                    deletion_job_id: r.deletion_job_id,
                    created_by: r.created_by,
                    version: r.version,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                },
                favorite: r.favorite,
            })
            .collect()
        }
        Some(ids) => {
            sqlx::query!(
                r#"select p.id, p.key, p.name, p.description, p.description_html, p.kind, p.template, p.owner_id,
                          p.logo_color, p.logo_key, p.next_number, p.default_audience_group_id, p.archived_at,
                          p.deleting_at, p.deletion_job_id, p.created_by, p.version, p.created_at,
                          p.updated_at,
                          f.user_id is not null as "favorite!",
                          case when f.user_id is not null and $3 then 0 else 1 end as "rank!"
                   from projects p
                   left join project_favorites f on f.project_id = p.id and f.user_id = $1
                   where p.id = any($10)
                     and (p.archived_at is not null) = $4 and (p.deleting_at is not null) = $5
                     and (strpos(lower(p.name), lower($2)) > 0 or starts_with(p.key, upper($2)))
                     and (case when f.user_id is not null and $3 then 0 else 1 end,
                          lower(p.name), p.id) > ($6, lower($7), $8)
                   order by case when f.user_id is not null and $3 then 0 else 1 end, lower(p.name), p.id
                   limit $9"#,
                viewer,
                text,
                stars,
                archived,
                deleting,
                after.0,
                after.1,
                after.2,
                limit,
                ids,
            )
            .fetch_all(conn)
            .await?
            .into_iter()
            .map(|r| Listed {
                project: Project {
                    id: r.id,
                    key: r.key,
                    name: r.name,
                    description: r.description,
                    description_html: r.description_html,
                    kind: r.kind,
                    template: r.template,
                    owner_id: r.owner_id,
                    logo_color: r.logo_color,
                    logo_key: r.logo_key,
                    next_number: r.next_number,
                    default_audience_group_id: r.default_audience_group_id,
                    archived_at: r.archived_at,
                    deleting_at: r.deleting_at,
                    deletion_job_id: r.deletion_job_id,
                    created_by: r.created_by,
                    version: r.version,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                },
                favorite: r.favorite,
            })
            .collect()
        }
    };
    Ok(rows)
}

/// Stars a project for a person (PRJ-012); `false` when it was starred already.
pub async fn add_favorite(
    conn: &mut PgConnection,
    user: UserId,
    project: ProjectId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "insert into project_favorites (user_id, project_id) values ($1, $2)
         on conflict do nothing",
        user,
        project
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn remove_favorite(
    conn: &mut PgConnection,
    user: UserId,
    project: ProjectId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "delete from project_favorites where user_id = $1 and project_id = $2",
        user,
        project
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Which of these projects a person starred.
pub async fn favorites_among(
    db: impl PgExecutor<'_>,
    user: UserId,
    ids: &[ProjectId],
) -> Result<Vec<ProjectId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select project_id from project_favorites where user_id = $1 and project_id = any($2)",
        user,
        ids
    )
    .fetch_all(db)
    .await
}

/// The project with this name, ignoring case (PRJ-002).
pub async fn with_name(
    db: impl PgExecutor<'_>,
    name: &str,
) -> Result<Option<ProjectId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select id from projects where lower(name) = lower($1)",
        name
    )
    .fetch_optional(db)
    .await
}

/// Removes the stars of deleted accounts.
pub async fn remove_favorites_of(
    conn: &mut PgConnection,
    users: &[UserId],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "delete from project_favorites where user_id = any($1)",
        users
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Sets or clears a project's uploaded logo (PRJ-007); answers the key it replaced.
pub async fn set_logo_key(
    conn: &mut PgConnection,
    id: ProjectId,
    key: Option<&str>,
) -> Result<Option<String>, sqlx::Error> {
    let old = sqlx::query_scalar!("select logo_key from projects where id = $1 for update", id)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
    sqlx::query!("update projects set logo_key = $2 where id = $1", id, key)
        .execute(conn)
        .await?;
    Ok(old)
}
