//! Value sets and their values (FLD-029…FLD-049): named lists of allowed values that fields and
//! projects share or own. User sets list accounts and groups instead of values (FLD-040).

use sqlx::{PgConnection, PgExecutor};
use taskmaster_domain::{FieldId, GroupId, ProjectId, SetId, UserId, ValueId};
use time::{Date, OffsetDateTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueSet {
    pub id: SetId,
    pub name: String,
    /// `enum`, `state`, `user`, `version`, `build` or `owned`.
    pub kind: String,
    pub version: i64,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub id: ValueId,
    pub set_id: SetId,
    pub name: String,
    pub description: String,
    pub color: Option<i16>,
    pub rank: String,
    pub archived: bool,
    pub resolved: bool,
    pub released: bool,
    pub release_date: Option<Date>,
    pub start_date: Option<Date>,
    pub assembled_on: Option<Date>,
    pub owner_id: Option<UserId>,
    pub version: i64,
    pub created_at: OffsetDateTime,
}

/// Unique constraints; a violation names one of these.
pub const SET_NAME_KEY: &str = "value_sets_name_key";
pub const VALUE_NAME_KEY: &str = "field_values_name_key";

pub async fn insert(conn: &mut PgConnection, name: &str, kind: &str) -> Result<SetId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into value_sets (name, kind) values ($1, $2) returning id",
        name,
        kind
    )
    .fetch_one(conn)
    .await
}

/// Names of sets of a kind starting with a prefix, for choosing a free name.
pub async fn names_like(
    db: impl PgExecutor<'_>,
    kind: &str,
    prefix: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "select lower(name) as \"name!\" from value_sets
         where kind = $1 and starts_with(lower(name), lower($2))",
        kind,
        prefix
    )
    .fetch_all(db)
    .await
}

pub async fn get(db: impl PgExecutor<'_>, id: SetId) -> Result<Option<ValueSet>, sqlx::Error> {
    sqlx::query_as!(
        ValueSet,
        "select id, name, kind, version, created_at from value_sets where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(
    db: impl PgExecutor<'_>,
    ids: &[SetId],
) -> Result<Vec<ValueSet>, sqlx::Error> {
    sqlx::query_as!(
        ValueSet,
        "select id, name, kind, version, created_at from value_sets where id = any($1)",
        ids
    )
    .fetch_all(db)
    .await
}

pub async fn lock(conn: &mut PgConnection, id: SetId) -> Result<Option<ValueSet>, sqlx::Error> {
    sqlx::query_as!(
        ValueSet,
        "select id, name, kind, version, created_at from value_sets where id = $1 for update",
        id
    )
    .fetch_optional(conn)
    .await
}

/// A page of sets by name after `(lowercased name, id)`; `text` matches a part of the name.
pub async fn list(
    conn: &mut PgConnection,
    text: &str,
    after: (&str, SetId),
    limit: i64,
) -> Result<Vec<ValueSet>, sqlx::Error> {
    sqlx::query_as!(
        ValueSet,
        "select id, name, kind, version, created_at from value_sets
         where strpos(lower(name), lower($1)) > 0 and (lower(name), id) > ($2, $3)
         order by lower(name), id
         limit $4",
        text,
        after.0,
        after.1,
        limit
    )
    .fetch_all(conn)
    .await
}

/// The same, for one kind of set.
pub async fn list_of_kind(
    conn: &mut PgConnection,
    kind: &str,
    text: &str,
    after: (&str, SetId),
    limit: i64,
) -> Result<Vec<ValueSet>, sqlx::Error> {
    sqlx::query_as!(
        ValueSet,
        "select id, name, kind, version, created_at from value_sets
         where kind = $1 and strpos(lower(name), lower($2)) > 0 and (lower(name), id) > ($3, $4)
         order by lower(name), id
         limit $5",
        kind,
        text,
        after.0,
        after.1,
        limit
    )
    .fetch_all(conn)
    .await
}

pub async fn rename(conn: &mut PgConnection, id: SetId, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update value_sets set name = $2, version = version + 1, updated_at = now() where id = $1",
        id,
        name
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Bumps the version after a change of its values or members.
pub async fn touch(conn: &mut PgConnection, ids: &[SetId]) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update value_sets set version = version + 1, updated_at = now() where id = any($1)",
        ids
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Who uses each set: projects that attach a field with it, and fields that offer it to new
/// attachments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    pub projects: Vec<ProjectId>,
    pub fields: Vec<FieldId>,
}

pub async fn usage(db: impl PgExecutor<'_>, id: SetId) -> Result<Usage, sqlx::Error> {
    let row = sqlx::query!(
        r#"select array(select distinct project_id from project_fields where set_id = $1
                        order by project_id) as "projects!",
                  array(select id from fields where set_id = $1 order by id) as "fields!""#,
        id
    )
    .fetch_one(db)
    .await?;
    Ok(Usage {
        projects: row.projects,
        fields: row.fields,
    })
}

/// How many projects and fields use each set: `(set, projects, fields)`.
pub async fn usage_counts(
    db: impl PgExecutor<'_>,
    ids: &[SetId],
) -> Result<Vec<(SetId, i64, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select s.id,
                  (select count(distinct project_id) from project_fields p where p.set_id = s.id)
                    as "projects!",
                  (select count(*) from fields f where f.set_id = s.id) as "fields!"
           from value_sets s where s.id = any($1)"#,
        ids
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.id, r.projects, r.fields))
        .collect())
}

/// Deletes sets that no field and no project uses any more; returns them.
pub async fn delete_unused(
    conn: &mut PgConnection,
    ids: &[SetId],
) -> Result<Vec<SetId>, sqlx::Error> {
    sqlx::query_scalar!(
        "delete from value_sets s where s.id = any($1)
           and not exists(select 1 from fields f where f.set_id = s.id)
           and not exists(select 1 from project_fields p where p.set_id = s.id)
         returning s.id",
        ids
    )
    .fetch_all(conn)
    .await
}

/// A value to create. Values are already validated.
#[derive(Debug, Clone, Default)]
pub struct NewValue<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub color: Option<i16>,
    pub rank: &'a str,
    pub resolved: bool,
    pub released: bool,
    pub release_date: Option<Date>,
    pub start_date: Option<Date>,
    pub assembled_on: Option<Date>,
    pub owner_id: Option<UserId>,
}

pub async fn insert_value(
    conn: &mut PgConnection,
    set: SetId,
    value: &NewValue<'_>,
) -> Result<ValueId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into field_values (set_id, name, description, color, rank, resolved, released,
                                   release_date, start_date, assembled_on, owner_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) returning id",
        set,
        value.name,
        value.description,
        value.color,
        value.rank,
        value.resolved,
        value.released,
        value.release_date,
        value.start_date,
        value.assembled_on,
        value.owner_id,
    )
    .fetch_one(conn)
    .await
}

/// Copies every value of one set into another, in the same order; returns `(old, new)` IDs.
pub async fn copy_values(
    conn: &mut PgConnection,
    from: SetId,
    to: SetId,
) -> Result<Vec<(ValueId, ValueId)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"with source as (
             select id, name, description, color, rank, archived, resolved, released,
                    release_date, start_date, assembled_on, owner_id
             from field_values where set_id = $1
           ),
           inserted as (
             insert into field_values (set_id, name, description, color, rank, archived,
                                       resolved, released, release_date, start_date,
                                       assembled_on, owner_id)
             select $2, name, description, color, rank, archived, resolved, released,
                    release_date, start_date, assembled_on, owner_id
             from source order by rank
             returning id, name
           )
           select s.id as "old!", i.id as "new!"
           from source s join inserted i on lower(i.name) = lower(s.name)"#,
        from,
        to
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.old, r.new)).collect())
}

/// Values of sets, each set in its order.
pub async fn values_of(db: impl PgExecutor<'_>, sets: &[SetId]) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_as!(
        Value,
        "select id, set_id, name, description, color, rank, archived, resolved, released,
                release_date, start_date, assembled_on, owner_id, version, created_at
         from field_values where set_id = any($1)
         order by set_id, rank",
        sets
    )
    .fetch_all(db)
    .await
}

pub async fn get_values(
    db: impl PgExecutor<'_>,
    ids: &[ValueId],
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_as!(
        Value,
        "select id, set_id, name, description, color, rank, archived, resolved, released,
                release_date, start_date, assembled_on, owner_id, version, created_at
         from field_values where id = any($1)",
        ids
    )
    .fetch_all(db)
    .await
}

pub async fn lock_value(
    conn: &mut PgConnection,
    id: ValueId,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_as!(
        Value,
        "select id, set_id, name, description, color, rank, archived, resolved, released,
                release_date, start_date, assembled_on, owner_id, version, created_at
         from field_values where id = $1 for update",
        id
    )
    .fetch_optional(conn)
    .await
}

pub async fn value_count(db: impl PgExecutor<'_>, set: SetId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(*) as "count!" from field_values where set_id = $1"#,
        set
    )
    .fetch_one(db)
    .await
}

/// The ranks next to a value: the last rank before it and the first after it, for placing
/// another value right after it.
pub async fn rank_after(
    db: impl PgExecutor<'_>,
    set: SetId,
    rank: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select min(rank collate "C") as "rank" from field_values
           where set_id = $1 and rank > $2::text collate "C""#,
        set,
        rank
    )
    .fetch_one(db)
    .await
}

/// The first and the last rank of a set.
pub async fn rank_bounds(
    db: impl PgExecutor<'_>,
    set: SetId,
) -> Result<(Option<String>, Option<String>), sqlx::Error> {
    let row = sqlx::query!(
        r#"select min(rank collate "C") as "first", max(rank collate "C") as "last"
           from field_values where set_id = $1"#,
        set
    )
    .fetch_one(db)
    .await?;
    Ok((row.first, row.last))
}

/// Changes of a value; `None` keeps a property. Dates and the owner take `Some(None)` to clear.
#[derive(Debug, Clone, Default)]
pub struct ValueChange<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub color: Option<Option<i16>>,
    pub archived: Option<bool>,
    pub resolved: Option<bool>,
    pub released: Option<bool>,
    pub release_date: Option<Option<Date>>,
    pub start_date: Option<Option<Date>>,
    pub assembled_on: Option<Option<Date>>,
    pub owner_id: Option<Option<UserId>>,
    pub rank: Option<&'a str>,
}

pub async fn update_value(
    conn: &mut PgConnection,
    id: ValueId,
    change: &ValueChange<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update field_values set
           name = coalesce($2, name),
           description = coalesce($3, description),
           color = case when $4 then $5 else color end,
           archived = coalesce($6, archived),
           resolved = coalesce($7, resolved),
           released = coalesce($8, released),
           release_date = case when $9 then $10 else release_date end,
           start_date = case when $11 then $12 else start_date end,
           assembled_on = case when $13 then $14 else assembled_on end,
           owner_id = case when $15 then $16 else owner_id end,
           rank = coalesce($17, rank),
           version = version + 1,
           updated_at = now()
         where id = $1",
        id,
        change.name,
        change.description,
        change.color.is_some(),
        change.color.flatten(),
        change.archived,
        change.resolved,
        change.released,
        change.release_date.is_some(),
        change.release_date.flatten(),
        change.start_date.is_some(),
        change.start_date.flatten(),
        change.assembled_on.is_some(),
        change.assembled_on.flatten(),
        change.owner_id.is_some(),
        change.owner_id.flatten(),
        change.rank,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_value(conn: &mut PgConnection, id: ValueId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from field_values where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Moves values owned by deleted accounts to their replacement (architecture.md §6).
pub async fn replace_owner(
    conn: &mut PgConnection,
    users: &[UserId],
    replacement: Option<UserId>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update field_values set owner_id = $2, version = version + 1, updated_at = now()
         where owner_id = any($1)",
        users,
        replacement
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Accounts and groups of user sets: `(set, account)` and `(set, group)` pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Members {
    pub users: Vec<(SetId, UserId)>,
    pub groups: Vec<(SetId, GroupId)>,
}

pub async fn members(db: &mut PgConnection, sets: &[SetId]) -> Result<Members, sqlx::Error> {
    let users = sqlx::query!(
        "select v.set_id, v.user_id from value_set_users v join users u on u.id = v.user_id
         where v.set_id = any($1) order by lower(u.full_name), u.id",
        sets
    )
    .fetch_all(&mut *db)
    .await?;
    let groups = sqlx::query!(
        "select v.set_id, v.group_id from value_set_groups v join groups g on g.id = v.group_id
         where v.set_id = any($1) order by g.builtin is null, lower(g.name), g.id",
        sets
    )
    .fetch_all(db)
    .await?;
    Ok(Members {
        users: users.into_iter().map(|r| (r.set_id, r.user_id)).collect(),
        groups: groups.into_iter().map(|r| (r.set_id, r.group_id)).collect(),
    })
}

/// Adds accounts to a user set; returns those actually added.
pub async fn add_users(
    conn: &mut PgConnection,
    set: SetId,
    users: &[UserId],
) -> Result<Vec<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into value_set_users (set_id, user_id) select $1, unnest($2::bigint[])
         on conflict do nothing returning user_id",
        set,
        users
    )
    .fetch_all(conn)
    .await
}

pub async fn remove_users(
    conn: &mut PgConnection,
    set: SetId,
    users: &[UserId],
) -> Result<Vec<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        "delete from value_set_users where set_id = $1 and user_id = any($2) returning user_id",
        set,
        users
    )
    .fetch_all(conn)
    .await
}

pub async fn add_groups(
    conn: &mut PgConnection,
    set: SetId,
    groups: &[GroupId],
) -> Result<Vec<GroupId>, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into value_set_groups (set_id, group_id) select $1, unnest($2::bigint[])
         on conflict do nothing returning group_id",
        set,
        groups
    )
    .fetch_all(conn)
    .await
}

pub async fn remove_groups(
    conn: &mut PgConnection,
    set: SetId,
    groups: &[GroupId],
) -> Result<Vec<GroupId>, sqlx::Error> {
    sqlx::query_scalar!(
        "delete from value_set_groups where set_id = $1 and group_id = any($2) returning group_id",
        set,
        groups
    )
    .fetch_all(conn)
    .await
}

/// Number of accounts and groups listed in a user set.
pub async fn member_counts(db: impl PgExecutor<'_>, set: SetId) -> Result<(i64, i64), sqlx::Error> {
    let row = sqlx::query!(
        r#"select (select count(*) from value_set_users where set_id = $1) as "users!",
                  (select count(*) from value_set_groups where set_id = $1) as "groups!""#,
        set
    )
    .fetch_one(db)
    .await?;
    Ok((row.users, row.groups))
}

/// Copies the accounts and groups of one user set into another.
pub async fn copy_members(
    conn: &mut PgConnection,
    from: SetId,
    to: SetId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into value_set_users (set_id, user_id)
         select $2, user_id from value_set_users where set_id = $1 on conflict do nothing",
        from,
        to
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "insert into value_set_groups (set_id, group_id)
         select $2, group_id from value_set_groups where set_id = $1 on conflict do nothing",
        from,
        to
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes deleted accounts from every user set (architecture.md §6).
pub async fn remove_users_everywhere(
    conn: &mut PgConnection,
    users: &[UserId],
) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from value_set_users where user_id = any($1)", users)
        .execute(conn)
        .await?;
    Ok(())
}

/// Puts the replacement of deleted groups into every user set that listed one (ACC-032).
pub async fn replace_groups(
    conn: &mut PgConnection,
    groups: &[GroupId],
    replacement: GroupId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into value_set_groups (set_id, group_id)
         select distinct set_id, $2::bigint from value_set_groups where group_id = any($1)
         on conflict do nothing",
        groups,
        replacement
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Whether an account may be picked from a user set: listed, or in a listed group (team groups
/// included).
pub async fn user_allowed(
    db: impl PgExecutor<'_>,
    set: SetId,
    user: UserId,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"with person as (
             select id, builtin is distinct from 'guest' as member from users where id = $2
           ),
           listed as (
             select g.id from value_set_groups v join groups g on g.id = v.group_id
             where v.set_id = $1
             union
             select t.group_id from value_set_groups v join team_groups t on t.team_id = v.group_id
             where v.set_id = $1
           )
           select exists(
             select 1 from value_set_users where set_id = $1 and user_id = $2
             union all
             select 1 from listed l join groups g on g.id = l.id, person p
             where g.builtin = 'everyone'
                or (g.builtin = 'all_members' and p.member)
                or exists(select 1 from group_members m where m.group_id = g.id and m.user_id = p.id)
           ) as "allowed!""#,
        set,
        user
    )
    .fetch_one(db)
    .await
}

/// Values a field of this set may take (FLD-008…FLD-014): not archived, whose folded name holds
/// `text` (already folded), in the set's order. `limit` rows at most.
pub async fn options(
    db: impl PgExecutor<'_>,
    set: SetId,
    text: &str,
    limit: i64,
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_as!(
        Value,
        "select id, set_id, name, description, color, rank, archived, resolved, released,
                release_date, start_date, assembled_on, owner_id, version, created_at
         from field_values
         where set_id = $1 and not archived
           and strpos(replace(lower(name), 'ё', 'е'), $2) > 0
         order by rank
         limit $3",
        set,
        text,
        limit
    )
    .fetch_all(db)
    .await
}

/// Accounts a user set offers (FLD-040): listed ones and members of listed groups and teams, as
/// [`user_allowed`] reads them; active, not the guest; whose folded full name or username holds
/// `text`. By name, with `first` ahead when they qualify. `limit` rows at most.
pub async fn user_options(
    db: impl PgExecutor<'_>,
    set: SetId,
    text: &str,
    first: UserId,
    limit: i64,
) -> Result<Vec<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"with listed as (
             select g.id, g.builtin from value_set_groups v join groups g on g.id = v.group_id
             where v.set_id = $1
             union
             select g.id, g.builtin from value_set_groups v
               join team_groups t on t.team_id = v.group_id
               join groups g on g.id = t.group_id
             where v.set_id = $1
           ),
           allowed as (
             select user_id as id from value_set_users where set_id = $1
             union
             select m.user_id from listed l join group_members m on m.group_id = l.id
             union
             select u.id from users u
             where exists(select 1 from listed where builtin in ('everyone', 'all_members'))
           )
           select u.id from allowed a join users u on u.id = a.id
           where u.deleted_at is null and u.disabled_at is null
             and u.builtin is distinct from 'guest'
             and (strpos(replace(lower(u.full_name), 'ё', 'е'), $2) > 0
                  or strpos(lower(u.username), $2) > 0)
           order by u.id <> $3, lower(u.full_name), u.id
           limit $4"#,
        set,
        text,
        first,
        limit
    )
    .fetch_all(db)
    .await
}
