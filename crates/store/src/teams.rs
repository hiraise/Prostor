//! Project teams (PRJ-047, ACC-033): a group that belongs to its project. Its users are explicit
//! group members (`groups::add_members`); groups in the team are listed here and bring all their
//! current members.

use sqlx::{PgConnection, PgExecutor};
use trackflow_domain::{GroupId, ProjectId, UserId};

/// Creates the team of a new project, named after it.
pub async fn create(
    conn: &mut PgConnection,
    project: ProjectId,
    name: &str,
) -> Result<GroupId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into groups (name, team_of) values ($1, $2) returning id",
        name,
        project,
    )
    .fetch_one(conn)
    .await
}

/// The team of a project.
pub async fn of_project(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<Option<GroupId>, sqlx::Error> {
    sqlx::query_scalar!("select id from groups where team_of = $1", project)
        .fetch_optional(db)
        .await
}

/// Teams of projects: `(project, team)` pairs.
pub async fn of_projects(
    db: impl PgExecutor<'_>,
    projects: &[ProjectId],
) -> Result<Vec<(ProjectId, GroupId)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select team_of as "project!", id from groups where team_of = any($1)"#,
        projects
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.project, r.id)).collect())
}

/// Renames a team with its project.
pub async fn rename(
    conn: &mut PgConnection,
    project: ProjectId,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update groups set name = $2, version = version + 1, updated_at = now()
         where team_of = $1",
        project,
        name
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The groups in a team, by name.
pub async fn groups(db: impl PgExecutor<'_>, team: GroupId) -> Result<Vec<GroupId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select t.group_id from team_groups t join groups g on g.id = t.group_id
         where t.team_id = $1 order by g.builtin is null, lower(g.name), g.id",
        team
    )
    .fetch_all(db)
    .await
}

/// Adds groups to a team; returns those actually added.
pub async fn add_groups(
    conn: &mut PgConnection,
    team: GroupId,
    groups: &[GroupId],
) -> Result<Vec<GroupId>, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into team_groups (team_id, group_id) select $1, unnest($2::bigint[])
         on conflict do nothing returning group_id",
        team,
        groups
    )
    .fetch_all(conn)
    .await
}

/// Removes groups from a team; returns those actually removed.
pub async fn remove_groups(
    conn: &mut PgConnection,
    team: GroupId,
    groups: &[GroupId],
) -> Result<Vec<GroupId>, sqlx::Error> {
    sqlx::query_scalar!(
        "delete from team_groups where team_id = $1 and group_id = any($2) returning group_id",
        team,
        groups
    )
    .fetch_all(conn)
    .await
}

/// Puts the replacement of deleted groups into every team that had one of them (ACC-032).
pub async fn replace_groups(
    conn: &mut PgConnection,
    groups: &[GroupId],
    replacement: GroupId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into team_groups (team_id, group_id)
         select distinct team_id, $2::bigint from team_groups
         where group_id = any($1) and team_id <> $2::bigint
         on conflict do nothing",
        groups,
        replacement
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// How many accounts each team has (PRJ-046).
pub async fn sizes(
    db: impl PgExecutor<'_>,
    teams: &[GroupId],
) -> Result<Vec<(GroupId, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select id, team_size(id) as "size!" from groups where id = any($1)"#,
        teams
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.size)).collect())
}

/// The projects whose teams include each account, directly or through a group (ACC-002):
/// `(user, project)` pairs.
pub async fn projects_of_users(
    db: impl PgExecutor<'_>,
    users: &[UserId],
) -> Result<Vec<(UserId, ProjectId)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"with people as (
             select u.id, u.builtin = 'guest' as guest from users u where u.id = any($1)
           ),
           mine as (
             select m.user_id, m.group_id from group_members m where m.user_id = any($1)
             union all
             select p.id, g.id from people p, groups g where g.builtin = 'everyone'
             union all
             select p.id, g.id from people p, groups g
             where g.builtin = 'all_members' and not coalesce(p.guest, false)
           )
           select distinct mine.user_id as "user_id!", g.team_of as "project!"
           from mine join groups g on g.id = mine.group_id
           where g.team_of is not null
           union
           select distinct mine.user_id, g.team_of
           from mine join team_groups t on t.group_id = mine.group_id
                     join groups g on g.id = t.team_id"#,
        users
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.user_id, r.project)).collect())
}

/// Whether an account is in a team, directly or through a group.
pub async fn includes(
    db: impl PgExecutor<'_>,
    team: GroupId,
    user: UserId,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select exists(
             select 1 from group_members where group_id = $1 and user_id = $2
             union all
             select 1 from team_groups t join groups g on g.id = t.group_id
             where t.team_id = $1
               and (g.builtin = 'everyone'
                    or (g.builtin = 'all_members'
                        and exists(select 1 from users u
                                   where u.id = $2 and u.builtin is distinct from 'guest'))
                    or exists(select 1 from group_members m
                              where m.group_id = g.id and m.user_id = $2))
           ) as "included!""#,
        team,
        user
    )
    .fetch_one(db)
    .await
}
