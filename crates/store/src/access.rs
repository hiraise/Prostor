//! Role assignments and capability groups (ACC-058…ACC-064): who holds which role where, and what
//! compiling a person's access needs to read.

use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::{GroupId, ProjectId, RoleId, UserId};

/// Who holds an assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Holder {
    User(UserId),
    Group(GroupId),
}

impl Holder {
    fn user(self) -> Option<UserId> {
        match self {
            Holder::User(id) => Some(id),
            Holder::Group(_) => None,
        }
    }

    fn group(self) -> Option<GroupId> {
        match self {
            Holder::User(_) => None,
            Holder::Group(id) => Some(id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub id: i64,
    pub role_id: RoleId,
    pub holder: Holder,
    /// `None` for global scope.
    pub project_id: Option<ProjectId>,
    pub created_at: OffsetDateTime,
    pub created_by: Option<UserId>,
}

/// An assignment with what compiling access needs from its role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRow {
    pub assignment: Assignment,
    pub role_builtin: Option<String>,
    pub role_permissions: Vec<String>,
}

struct Row {
    id: i64,
    role_id: RoleId,
    user_id: Option<UserId>,
    group_id: Option<GroupId>,
    project_id: Option<ProjectId>,
    created_at: OffsetDateTime,
    created_by: Option<UserId>,
}

impl Row {
    fn into_assignment(self) -> Option<Assignment> {
        let holder = match (self.user_id, self.group_id) {
            (Some(user), None) => Holder::User(user),
            (None, Some(group)) => Holder::Group(group),
            // The table's check constraint rules this out.
            _ => return None,
        };
        Some(Assignment {
            id: self.id,
            role_id: self.role_id,
            holder,
            project_id: self.project_id,
            created_at: self.created_at,
            created_by: self.created_by,
        })
    }
}

/// Assigns a role; returns the new assignment's ID, or `None` when the holder already has the role
/// at this scope.
pub async fn insert(
    conn: &mut PgConnection,
    role: RoleId,
    holder: Holder,
    project: Option<ProjectId>,
    by: Option<UserId>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into role_assignments (role_id, user_id, group_id, project_id, created_by)
         values ($1, $2, $3, $4, $5)
         on conflict do nothing returning id",
        role,
        holder.user(),
        holder.group(),
        project,
        by,
    )
    .fetch_optional(conn)
    .await
}

pub async fn lock(conn: &mut PgConnection, id: i64) -> Result<Option<Assignment>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        "select id, role_id, user_id, group_id, project_id, created_at, created_by
         from role_assignments where id = $1 for update",
        id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(Row::into_assignment))
}

pub async fn delete(conn: &mut PgConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from role_assignments where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Removes every direct assignment of these accounts (account deletion).
pub async fn delete_for_users(
    conn: &mut PgConnection,
    users: &[UserId],
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "delete from role_assignments where user_id = any($1)",
        users
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// Assignments held directly by an account or a group.
pub async fn of_holder(
    db: impl PgExecutor<'_>,
    holder: Holder,
) -> Result<Vec<Assignment>, sqlx::Error> {
    let rows = match holder {
        Holder::User(user) => {
            sqlx::query_as!(
                Row,
                "select id, role_id, user_id, group_id, project_id, created_at, created_by
                 from role_assignments where user_id = $1 order by id",
                user,
            )
            .fetch_all(db)
            .await?
        }
        Holder::Group(group) => {
            sqlx::query_as!(
                Row,
                "select id, role_id, user_id, group_id, project_id, created_at, created_by
                 from role_assignments where group_id = $1 order by id",
                group,
            )
            .fetch_all(db)
            .await?
        }
    };
    Ok(rows.into_iter().filter_map(Row::into_assignment).collect())
}

/// Assignments held by any of these groups.
pub async fn of_groups(
    db: impl PgExecutor<'_>,
    groups: &[GroupId],
) -> Result<Vec<Assignment>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        "select id, role_id, user_id, group_id, project_id, created_at, created_by
         from role_assignments where group_id = any($1) order by id",
        groups,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(Row::into_assignment).collect())
}

/// Every assignment of a role, for showing what deleting it revokes (ACC-057).
pub async fn of_role(
    db: impl PgExecutor<'_>,
    role: RoleId,
) -> Result<Vec<Assignment>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        "select id, role_id, user_id, group_id, project_id, created_at, created_by
         from role_assignments where role_id = $1 order by id",
        role,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(Row::into_assignment).collect())
}

/// How many holders each role has, for the roles list.
pub async fn counts_by_role(db: impl PgExecutor<'_>) -> Result<Vec<(RoleId, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select role_id, count(*) as "count!" from role_assignments group by role_id"#
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.role_id, row.count))
        .collect())
}

/// Everything compiling one person's access needs: the assignments held by the person and by
/// their groups, with each role's permissions. One query.
pub async fn grants(
    db: impl PgExecutor<'_>,
    user: UserId,
    groups: &[GroupId],
) -> Result<Vec<GrantRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        GrantSql,
        "select a.id, a.role_id, a.user_id, a.group_id, a.project_id, a.created_at, a.created_by,
                r.builtin, r.permissions
         from role_assignments a join roles r on r.id = a.role_id
         where a.user_id = $1 or a.group_id = any($2)
         order by a.id",
        user,
        groups,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(GrantSql::into_row).collect())
}

/// Everything compiling several people's access needs: the assignments held by any of them or by
/// any of their groups. One query; each person takes the rows of their own and their groups.
pub async fn grants_of_users(
    db: impl PgExecutor<'_>,
    users: &[UserId],
    groups: &[GroupId],
) -> Result<Vec<GrantRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        GrantSql,
        "select a.id, a.role_id, a.user_id, a.group_id, a.project_id, a.created_at, a.created_by,
                r.builtin, r.permissions
         from role_assignments a join roles r on r.id = a.role_id
         where a.user_id = any($1) or a.group_id = any($2)
         order by a.id",
        users,
        groups,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(GrantSql::into_row).collect())
}

/// The assignments of these groups with their roles' permissions: what joining them gives.
pub async fn grants_of_groups(
    db: impl PgExecutor<'_>,
    groups: &[GroupId],
) -> Result<Vec<GrantRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        GrantSql,
        "select a.id, a.role_id, a.user_id, a.group_id, a.project_id, a.created_at, a.created_by,
                r.builtin, r.permissions
         from role_assignments a join roles r on r.id = a.role_id
         where a.group_id = any($1)
         order by a.id",
        groups,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(GrantSql::into_row).collect())
}

struct GrantSql {
    id: i64,
    role_id: RoleId,
    user_id: Option<UserId>,
    group_id: Option<GroupId>,
    project_id: Option<ProjectId>,
    created_at: OffsetDateTime,
    created_by: Option<UserId>,
    builtin: Option<String>,
    permissions: Vec<String>,
}

impl GrantSql {
    fn into_row(self) -> Option<GrantRow> {
        let assignment = Row {
            id: self.id,
            role_id: self.role_id,
            user_id: self.user_id,
            group_id: self.group_id,
            project_id: self.project_id,
            created_at: self.created_at,
            created_by: self.created_by,
        }
        .into_assignment()?;
        Some(GrantRow {
            assignment,
            role_builtin: self.builtin,
            role_permissions: self.permissions,
        })
    }
}

/// Whether some enabled account other than the guest still holds the system administrator role
/// globally, directly or through a group. Checked inside every change that could take the last
/// one away, before it commits.
pub async fn administrator_remains(conn: &mut PgConnection) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"with active as (
             select id from users
             where disabled_at is null and deleted_at is null and builtin is distinct from 'guest'
           )
           select exists(
             select 1 from role_assignments a join roles r on r.id = a.role_id
             left join groups g on g.id = a.group_id
             where r.builtin = 'system_admin' and a.project_id is null
               and (a.user_id in (select id from active)
                    or exists(select 1 from group_members m
                              where m.group_id = a.group_id
                                and m.user_id in (select id from active))
                    or (g.builtin in ('everyone', 'all_members')
                        and exists(select 1 from active)))
           ) as "remains!""#
    )
    .fetch_one(conn)
    .await
}

/// The groups of each capability: `(capability key, group)` pairs.
pub async fn capability_groups(
    db: impl PgExecutor<'_>,
) -> Result<Vec<(String, GroupId)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select capability, group_id from capability_groups order by capability, group_id"
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.capability, row.group_id))
        .collect())
}

/// The keys of capabilities that are on for any of these groups.
pub async fn capabilities_of(
    db: impl PgExecutor<'_>,
    groups: &[GroupId],
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "select distinct capability from capability_groups where group_id = any($1)",
        groups,
    )
    .fetch_all(db)
    .await
}

/// The capabilities of each of these groups: `(group, capability key)` pairs.
pub async fn capabilities_of_groups(
    db: impl PgExecutor<'_>,
    groups: &[GroupId],
) -> Result<Vec<(GroupId, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select group_id, capability from capability_groups where group_id = any($1)",
        groups,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.group_id, row.capability))
        .collect())
}

/// Replaces the groups of a capability; returns the previous ones.
pub async fn set_capability_groups(
    conn: &mut PgConnection,
    capability: &str,
    groups: &[GroupId],
) -> Result<Vec<GroupId>, sqlx::Error> {
    let old = sqlx::query_scalar!(
        "delete from capability_groups where capability = $1 returning group_id",
        capability,
    )
    .fetch_all(&mut *conn)
    .await?;
    sqlx::query!(
        "insert into capability_groups (capability, group_id)
         select $1, unnest($2::bigint[]) on conflict do nothing",
        capability,
        groups,
    )
    .execute(conn)
    .await?;
    Ok(old)
}

/// Every assignment in one project: team roles and direct roles (PRJ-046).
pub async fn of_project(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<Vec<Assignment>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        "select id, role_id, user_id, group_id, project_id, created_at, created_by
         from role_assignments where project_id = $1 order by id",
        project,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(Row::into_assignment).collect())
}

/// Every global assignment; the people page shows those that reach projects (PRJ-046).
pub async fn global(db: impl PgExecutor<'_>) -> Result<Vec<Assignment>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        "select id, role_id, user_id, group_id, project_id, created_at, created_by
         from role_assignments where project_id is null order by id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(Row::into_assignment).collect())
}

/// Whether a holder has a role at a scope.
pub async fn exists(
    db: impl PgExecutor<'_>,
    role: RoleId,
    holder: Holder,
    project: Option<ProjectId>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select exists(select 1 from role_assignments
                         where role_id = $1 and user_id is not distinct from $2
                           and group_id is not distinct from $3
                           and project_id is not distinct from $4) as "found!""#,
        role,
        holder.user(),
        holder.group(),
        project,
    )
    .fetch_one(db)
    .await
}
