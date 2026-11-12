//! Groups and their members (ACC-021…ACC-032). Members of the built-in groups are implicit
//! (`trackflow_domain::access::group::BuiltinGroup`): queries add them instead of storing a row per
//! account.

use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::{GroupId, ProjectId, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub description: String,
    /// `everyone` or `all_members` for the built-in groups.
    pub builtin: Option<String>,
    pub auto_join: bool,
    /// The project whose team this is (PRJ-047); its users and groups are managed there.
    pub team_of: Option<ProjectId>,
    pub version: i64,
    pub created_at: OffsetDateTime,
}

impl Group {
    pub fn is_team(&self) -> bool {
        self.team_of.is_some()
    }
}

/// A group of a list with its member count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub group: Group,
    pub members: i64,
}

/// Unique constraint on names; a violation names it.
pub const NAME_KEY: &str = "groups_name_key";

pub async fn insert(
    conn: &mut PgConnection,
    name: &str,
    description: &str,
    auto_join: bool,
) -> Result<GroupId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into groups (name, description, auto_join) values ($1, $2, $3) returning id",
        name,
        description,
        auto_join,
    )
    .fetch_one(conn)
    .await
}

pub async fn get(db: impl PgExecutor<'_>, id: GroupId) -> Result<Option<Group>, sqlx::Error> {
    sqlx::query_as!(
        Group,
        "select id, name, description, builtin, auto_join, team_of, version, created_at
         from groups where id = $1",
        id,
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(db: impl PgExecutor<'_>, ids: &[GroupId]) -> Result<Vec<Group>, sqlx::Error> {
    sqlx::query_as!(
        Group,
        "select id, name, description, builtin, auto_join, team_of, version, created_at
         from groups where id = any($1)",
        ids,
    )
    .fetch_all(db)
    .await
}

/// Groups locked for a change, in ID order.
pub async fn lock_many(
    conn: &mut PgConnection,
    ids: &[GroupId],
) -> Result<Vec<Group>, sqlx::Error> {
    sqlx::query_as!(
        Group,
        "select id, name, description, builtin, auto_join, team_of, version, created_at
         from groups where id = any($1) order by id for update",
        ids,
    )
    .fetch_all(conn)
    .await
}

/// The ID a group has now: itself, or the group that replaced a deleted one (ACC-013).
pub async fn resolve(conn: &mut PgConnection, id: GroupId) -> Result<Option<GroupId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select id as "id!" from groups where id = $1
           union all
           select group_id from group_aliases where old_id = $1
           limit 1"#,
        id,
    )
    .fetch_optional(conn)
    .await
}

/// IDs of the built-in groups: `(everyone, all_members)`.
pub async fn builtin_ids(db: impl PgExecutor<'_>) -> Result<(GroupId, GroupId), sqlx::Error> {
    let row = sqlx::query!(
        r#"select (select id from groups where builtin = 'everyone') as "everyone!",
                  (select id from groups where builtin = 'all_members') as "all_members!""#
    )
    .fetch_one(db)
    .await?;
    Ok((row.everyone, row.all_members))
}

/// Which groups a list shows (ACC-022).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kinds {
    /// Groups of people, the built-in ones included.
    pub groups: bool,
    /// Project teams.
    pub teams: bool,
}

/// Groups by name, built-in ones first, then by name and ID; after `(rank, name, id)` of the
/// previous page. `text` is a lowercased part of the name; `builtin` lists built-in keys whose
/// name in another language matches it; `teams_of` limits teams to these projects (`None`: all).
#[allow(clippy::too_many_arguments)]
pub async fn list(
    conn: &mut PgConnection,
    text: &str,
    builtin: &[String],
    kinds: Kinds,
    teams_of: Option<&[ProjectId]>,
    after: (i32, &str, GroupId),
    limit: i64,
) -> Result<Vec<Listed>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select g.id, g.name, g.description, g.builtin, g.auto_join, g.team_of, g.version,
                  g.created_at,
                  case when g.builtin is null then 1 else 0 end as "rank!",
                  case
                    when g.builtin = 'everyone'
                      then (select count(*) from users where deleted_at is null)
                    when g.builtin = 'all_members' then (select count(*) from users
                                                         where deleted_at is null
                                                           and builtin is distinct from 'guest')
                    when g.team_of is not null then team_size(g.id)
                    else (select count(*) from group_members m where m.group_id = g.id)
                  end as "members!"
           from groups g
           where (strpos(lower(g.name), lower($1)) > 0 or g.builtin = any($2))
             and ((g.team_of is null and $3)
                  or (g.team_of is not null and $4
                      and ($9::bigint[] is null or g.team_of = any($9))))
             and (case when g.builtin is null then 1 else 0 end, lower(g.name), g.id)
                 > ($5, lower($6), $7)
           order by case when g.builtin is null then 1 else 0 end, lower(g.name), g.id
           limit $8"#,
        text,
        builtin,
        kinds.groups,
        kinds.teams,
        after.0,
        after.1,
        after.2,
        limit,
        teams_of,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Listed {
            group: Group {
                id: row.id,
                name: row.name,
                description: row.description,
                builtin: row.builtin,
                auto_join: row.auto_join,
                team_of: row.team_of,
                version: row.version,
                created_at: row.created_at,
            },
            members: row.members,
        })
        .collect())
}

/// Member counts of groups: built-in ones counted from the accounts, teams with the members of
/// their groups.
pub async fn member_counts(
    db: impl PgExecutor<'_>,
    ids: &[GroupId],
) -> Result<Vec<(GroupId, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select g.id,
                  case
                    when g.builtin = 'everyone'
                      then (select count(*) from users where deleted_at is null)
                    when g.builtin = 'all_members' then (select count(*) from users
                                                         where deleted_at is null
                                                           and builtin is distinct from 'guest')
                    when g.team_of is not null then team_size(g.id)
                    else (select count(*) from group_members m where m.group_id = g.id)
                  end as "members!"
           from groups g where g.id = any($1)"#,
        ids,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|row| (row.id, row.members)).collect())
}

/// Changes name, description and auto-join; `None` keeps a value.
pub async fn update(
    conn: &mut PgConnection,
    id: GroupId,
    name: Option<&str>,
    description: Option<&str>,
    auto_join: Option<bool>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update groups set name = coalesce($2, name), description = coalesce($3, description),
                           auto_join = coalesce($4, auto_join), version = version + 1,
                           updated_at = now()
         where id = $1",
        id,
        name,
        description,
        auto_join,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes groups whose IDs now resolve to `replacement` (ACC-013, ACC-032). Older aliases of the
/// deleted groups move to the replacement, so every alias is one step.
pub async fn delete(
    conn: &mut PgConnection,
    ids: &[GroupId],
    replacement: GroupId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update group_aliases set group_id = $2 where group_id = any($1)",
        ids,
        replacement,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "insert into group_aliases (old_id, group_id) select unnest($1::bigint[]), $2",
        ids,
        replacement,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!("delete from groups where id = any($1)", ids)
        .execute(conn)
        .await?;
    Ok(())
}

/// Bumps the versions after a change stored elsewhere (members, roles).
pub async fn touch(conn: &mut PgConnection, ids: &[GroupId]) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update groups set version = version + 1, updated_at = now() where id = any($1)",
        ids
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Adds accounts to a group; existing members are skipped. Returns the IDs actually added.
pub async fn add_members(
    conn: &mut PgConnection,
    group: GroupId,
    users: &[UserId],
) -> Result<Vec<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into group_members (group_id, user_id) select $1, unnest($2::bigint[])
         on conflict do nothing returning user_id",
        group,
        users,
    )
    .fetch_all(conn)
    .await
}

/// Removes accounts from a group. Returns the IDs actually removed.
pub async fn remove_members(
    conn: &mut PgConnection,
    group: GroupId,
    users: &[UserId],
) -> Result<Vec<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        "delete from group_members where group_id = $1 and user_id = any($2) returning user_id",
        group,
        users,
    )
    .fetch_all(conn)
    .await
}

/// Removes accounts from every group (account deletion).
pub async fn remove_from_all(conn: &mut PgConnection, users: &[UserId]) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from group_members where user_id = any($1)", users)
        .execute(conn)
        .await?;
    Ok(())
}

/// Adds a new account to every group with auto-join (ACC-024).
pub async fn join_auto(conn: &mut PgConnection, user: UserId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into group_members (group_id, user_id)
         select id, $1 from groups where auto_join and builtin is null
         on conflict do nothing",
        user,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The explicit groups of each account, for directory rows: `(user, group)` pairs.
pub async fn memberships(
    db: impl PgExecutor<'_>,
    users: &[UserId],
) -> Result<Vec<(UserId, GroupId)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select user_id, group_id from group_members where user_id = any($1)",
        users
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.user_id, row.group_id))
        .collect())
}

/// Every group of one account: its explicit groups, the built-in groups it belongs to, and the
/// project teams that include any of these (PRJ-047).
pub async fn of_user(
    db: impl PgExecutor<'_>,
    user: UserId,
    guest: bool,
) -> Result<Vec<GroupId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"with mine as (
             select group_id as id from group_members where user_id = $1
             union all
             select id from groups where builtin = 'everyone'
             union all
             select id from groups where builtin = 'all_members' and not $2
           )
           select id as "id!" from mine
           union
           select t.team_id from team_groups t where t.group_id in (select id from mine)"#,
        user,
        guest,
    )
    .fetch_all(db)
    .await
}

/// Every group of several accounts, as [`of_user`] gives them for one: `(user, group)` pairs.
pub async fn of_users(
    db: impl PgExecutor<'_>,
    users: &[UserId],
) -> Result<Vec<(UserId, GroupId)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"with people as (
             select id, builtin is not distinct from 'guest' as guest from users
             where id = any($1)
           ),
           mine as (
             select m.user_id, m.group_id as id from group_members m where m.user_id = any($1)
             union all
             select p.id, g.id from people p cross join groups g where g.builtin = 'everyone'
             union all
             select p.id, g.id from people p cross join groups g
             where g.builtin = 'all_members' and not p.guest
           )
           select user_id as "user!", id as "group!" from mine
           union
           select m.user_id, t.team_id from team_groups t join mine m on t.group_id = m.id"#,
        users,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.user, r.group)).collect())
}

/// Members of an explicit group by name, after `(name, id)` of the previous page.
pub async fn members(
    conn: &mut PgConnection,
    group: GroupId,
    text: &str,
    after: (&str, UserId),
    limit: i64,
) -> Result<Vec<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select u.id from group_members m join users u on u.id = m.user_id
           where m.group_id = $1 and u.deleted_at is null
             and (strpos(lower(u.full_name), lower($2)) > 0 or strpos(lower(u.username), lower($2)) > 0)
             and (lower(u.full_name), u.id) > (lower($3), $4)
           order by lower(u.full_name), u.id
           limit $5"#,
        group,
        text,
        after.0,
        after.1,
        limit,
    )
    .fetch_all(conn)
    .await
}

/// Members of a built-in group (every account, or every account but the guest) by name.
pub async fn builtin_members(
    conn: &mut PgConnection,
    include_guest: bool,
    text: &str,
    after: (&str, UserId),
    limit: i64,
) -> Result<Vec<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select id from users
           where deleted_at is null and ($1 or builtin is distinct from 'guest')
             and (strpos(lower(full_name), lower($2)) > 0 or strpos(lower(username), lower($2)) > 0)
             and (lower(full_name), id) > (lower($3), $4)
           order by lower(full_name), id
           limit $5"#,
        include_guest,
        text,
        after.0,
        after.1,
        limit,
    )
    .fetch_all(conn)
    .await
}

/// Groups and teams whose folded name holds `text` (already folded), built-in ones first, then by
/// name. Teams only of `teams_of` when given. `limit` rows at most.
pub async fn named_like(
    db: impl PgExecutor<'_>,
    text: &str,
    teams_of: Option<&[ProjectId]>,
    limit: i64,
) -> Result<Vec<Group>, sqlx::Error> {
    sqlx::query_as!(
        Group,
        "select id, name, description, builtin, auto_join, team_of, version, created_at
         from groups
         where strpos(replace(lower(name), 'ё', 'е'), $1) > 0
           and (team_of is null or $2::bigint[] is null or team_of = any($2))
         order by builtin is null, lower(name), id
         limit $3",
        text,
        teams_of,
        limit
    )
    .fetch_all(db)
    .await
}
