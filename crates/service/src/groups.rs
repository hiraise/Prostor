//! Groups (ACC-021…ACC-032): the list, creating and editing, members, and deleting with a
//! replacement. Who may do what: ACC-051 (`Access::may_manage_groups`, `may_see_groups`); adding
//! members also needs every permission the group gives (ACC-050).

use serde_json::json;
use trackflow_domain::access::group::{
    BuiltinGroup, GROUP_DESCRIPTION_MAX, builtin_matching, group_name,
};
use trackflow_domain::access::permission::Permission;
use trackflow_domain::access::role::description;
use trackflow_domain::access::{Access, Projects};
use trackflow_domain::validation::{Problem, Problems};
use trackflow_domain::{GroupId, UserId};
use trackflow_store::access::{self as assignments, Assignment, Holder};
use trackflow_store::groups::{self, Group, Listed};
use trackflow_store::roles::{self, Role};
use trackflow_store::{PgPool, audit, projects, teams, unique_violation, users, value_sets};

use crate::access::{changed, grants_of_group, keep_an_administrator};
use crate::{Error, Result};

/// Most groups or accounts one action takes.
pub const BULK_MAX: usize = 100;
const SEARCH_MAX: usize = 100;

/// A group of the list with its own role assignments (not those of parent groups).
#[derive(Debug, Clone)]
pub struct GroupRow {
    pub listed: Listed,
    pub assignments: Vec<(Assignment, Role)>,
}

/// Pairs assignments with their roles.
async fn with_roles(
    conn: &mut sqlx::PgConnection,
    list: Vec<Assignment>,
) -> Result<Vec<(Assignment, Role)>> {
    let ids: Vec<i64> = list.iter().map(|a| a.role_id).collect();
    let found = roles::get_many(&mut *conn, &ids).await?;
    Ok(list
        .into_iter()
        .filter_map(|a| {
            let role = found.iter().find(|r| r.id == a.role_id)?.clone();
            Some((a, role))
        })
        .collect())
}

/// A page of groups, built-in ones first, then by name (ACC-022): groups of people, project teams
/// or both. `after` is the previous page's last `(rank, lowercased name, id)`.
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    text: &str,
    kinds: groups::Kinds,
    after: Option<(i32, String, GroupId)>,
    limit: i64,
) -> Result<Vec<GroupRow>> {
    viewer.may_see_groups()?;
    let text: String = text
        .trim()
        .to_lowercase()
        .chars()
        .take(SEARCH_MAX)
        .collect();
    let builtin: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        builtin_matching(&text)
            .into_iter()
            .map(|group| group.key().to_owned())
            .collect()
    };
    let (rank, name, id) = after.unwrap_or((-1, String::new(), 0));
    let mut conn = pool.acquire().await?;
    // Teams carry their project's name: only teams of projects the viewer may see (PRJ-009).
    let visible = match viewer.projects_with(Permission::ProjectBasic) {
        Projects::All => None,
        Projects::Only(ids) => Some(ids),
    };
    let rows = groups::list(
        &mut conn,
        &text,
        &builtin,
        kinds,
        visible.as_deref(),
        (rank, &name, id),
        limit,
    )
    .await?;
    let ids: Vec<GroupId> = rows.iter().map(|row| row.group.id).collect();
    let all = assignments::of_groups(&mut *conn, &ids).await?;
    let all = with_roles(&mut conn, all).await?;
    Ok(rows
        .into_iter()
        .map(|listed| {
            let assignments = all
                .iter()
                .filter(|(a, _)| a.holder == Holder::Group(listed.group.id))
                .cloned()
                .collect();
            GroupRow {
                listed,
                assignments,
            }
        })
        .collect())
}

/// One group with its members count and its own role assignments.
pub async fn get(pool: &PgPool, viewer: &Access, id: GroupId) -> Result<GroupRow> {
    viewer.may_see_groups()?;
    let mut conn = pool.acquire().await?;
    let id = groups::resolve(&mut conn, id)
        .await?
        .ok_or(Error::NotFound("no such group"))?;
    let group = groups::get(&mut *conn, id)
        .await?
        .filter(|group| team_visible(viewer, group))
        .ok_or(Error::NotFound("no such group"))?;
    let members = groups::member_counts(&mut *conn, &[id])
        .await?
        .first()
        .map_or(0, |(_, count)| *count);
    let assignments = assignments::of_holder(&mut *conn, Holder::Group(id)).await?;
    let assignments = with_roles(&mut conn, assignments).await?;
    Ok(GroupRow {
        listed: Listed { group, members },
        assignments,
    })
}

/// Groups by ID, for naming them in answers whose access was checked already.
pub async fn refs(pool: &PgPool, ids: &[GroupId]) -> Result<Vec<Group>> {
    Ok(groups::get_many(pool, ids).await?)
}

/// A group to create (ACC-023).
#[derive(Clone, Default)]
pub struct NewGroup {
    pub name: String,
    pub description: String,
    pub auto_join: bool,
}

/// Creates an empty group: no members, no roles (ACC-023).
pub async fn create(pool: &PgPool, actor: &Access, input: NewGroup) -> Result<Group> {
    actor.may_manage_groups()?;
    let mut problems = Problems::new();
    let name = problems.check("name", group_name(&input.name));
    let description = problems.check(
        "description",
        description(&input.description, GROUP_DESCRIPTION_MAX),
    );
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(description)) = (name, description) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    let mut tx = pool.begin().await?;
    let id = groups::insert(&mut tx, &name, &description, input.auto_join)
        .await
        .map_err(name_taken)?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "group.created",
        &format!("group:{id}"),
        json!({ "name": name, "auto_join": input.auto_join }),
    )
    .await?;
    tx.commit().await?;
    groups::get(pool, id)
        .await?
        .ok_or(Error::Internal("the new group is gone".to_owned()))
}

/// Changes of a group's settings (ACC-024); `None` keeps a value.
#[derive(Clone, Default)]
pub struct GroupChange {
    pub name: Option<String>,
    pub description: Option<String>,
    pub auto_join: Option<bool>,
    pub version: i64,
}

/// Changes name, description and auto-join. Built-in groups keep their name and auto-join.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    id: GroupId,
    change: GroupChange,
) -> Result<Group> {
    actor.may_manage_groups()?;
    let mut problems = Problems::new();
    let name = change
        .name
        .as_deref()
        .and_then(|raw| problems.check("name", group_name(raw)));
    let description = change
        .description
        .as_deref()
        .and_then(|raw| problems.check("description", description(raw, GROUP_DESCRIPTION_MAX)));
    problems.finish().map_err(Error::Invalid)?;
    let mut tx = pool.begin().await?;
    let group = groups::lock_many(&mut tx, &[id])
        .await?
        .pop()
        .ok_or(Error::NotFound("no such group"))?;
    if group.version != change.version {
        return Err(Error::Conflict("the group was changed by someone else"));
    }
    if group.is_team() {
        return Err(Error::Conflict("a project team changes with its project"));
    }
    if group.builtin.is_some() && (name.is_some() || change.auto_join.is_some()) {
        return Err(Error::Conflict(
            "built-in groups keep their name and auto-join",
        ));
    }
    groups::update(
        &mut tx,
        id,
        name.as_deref(),
        description.as_deref(),
        change.auto_join,
    )
    .await
    .map_err(name_taken)?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "group.updated",
        &format!("group:{id}"),
        json!({
            "name": name.as_ref().map(|new| json!({ "old": group.name, "new": new })),
            "description": description.as_ref().map(|new| json!({ "old": group.description, "new": new })),
            "auto_join": change.auto_join.map(|new| json!({ "old": group.auto_join, "new": new })),
        }),
    )
    .await?;
    tx.commit().await?;
    groups::get(pool, id)
        .await?
        .ok_or(Error::NotFound("no such group"))
}

/// A page of a group's members by name (built-in groups list every account they include).
pub async fn members(
    pool: &PgPool,
    viewer: &Access,
    id: GroupId,
    text: &str,
    after: Option<(String, UserId)>,
    limit: i64,
) -> Result<Vec<users::User>> {
    viewer.may_see_groups()?;
    let text: String = text
        .trim()
        .to_lowercase()
        .chars()
        .take(SEARCH_MAX)
        .collect();
    let (name, user) = after.unwrap_or_default();
    let mut conn = pool.acquire().await?;
    let group = groups::get(&mut *conn, id)
        .await?
        .filter(|group| team_visible(viewer, group))
        .ok_or(Error::NotFound("no such group"))?;
    let ids = match group.builtin.as_deref().and_then(BuiltinGroup::parse) {
        Some(builtin) => {
            groups::builtin_members(
                &mut conn,
                builtin.includes(true),
                &text,
                (&name, user),
                limit,
            )
            .await?
        }
        None => groups::members(&mut conn, id, &text, (&name, user), limit).await?,
    };
    let mut found = users::get_many(&mut *conn, &ids).await?;
    found.sort_by_key(|user| ids.iter().position(|id| *id == user.id));
    Ok(found)
}

/// Adds accounts to a group (ACC-026). The actor must hold everything the group gives (ACC-050)
/// unless they hold `system.write`. Built-in groups have implicit members.
pub async fn add_members(
    pool: &PgPool,
    actor: &Access,
    id: GroupId,
    users_to_add: &[UserId],
) -> Result<Vec<UserId>> {
    actor.may_manage_groups()?;
    let ids = bulk(users_to_add, "users")?;
    let mut tx = pool.begin().await?;
    let group = groups::lock_many(&mut tx, &[id])
        .await?
        .pop()
        .ok_or(Error::NotFound("no such group"))?;
    if group.builtin.is_some() {
        return Err(Error::Conflict(
            "built-in groups include accounts by themselves",
        ));
    }
    if group.is_team() {
        return Err(Error::Conflict(
            "a project team changes on its project's people page",
        ));
    }
    let grants = grants_of_group(&mut *tx, id).await?;
    actor.may_join_to(&grants)?;
    let found = users::get_many(&mut *tx, &ids).await?;
    if found.len() != ids.len() || found.iter().any(users::User::is_deleted) {
        return Err(Error::NotFound("no such account"));
    }
    let added = groups::add_members(&mut tx, id, &ids).await?;
    if !added.is_empty() {
        groups::touch(&mut tx, &[id]).await?;
        users::touch(&mut tx, &added).await?;
        audit::record(
            &mut tx,
            actor_of(actor),
            "group.members_added",
            &format!("group:{id}"),
            json!({ "users": added }),
        )
        .await?;
        changed(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(added)
}

/// Removes accounts from a group (ACC-026); never the last way an administrator is one.
pub async fn remove_members(
    pool: &PgPool,
    actor: &Access,
    id: GroupId,
    users_to_remove: &[UserId],
) -> Result<Vec<UserId>> {
    actor.may_manage_groups()?;
    let ids = bulk(users_to_remove, "users")?;
    let mut tx = pool.begin().await?;
    let group = groups::lock_many(&mut tx, &[id])
        .await?
        .pop()
        .ok_or(Error::NotFound("no such group"))?;
    if group.builtin.is_some() {
        return Err(Error::Conflict(
            "built-in groups include accounts by themselves",
        ));
    }
    if group.is_team() {
        return Err(Error::Conflict(
            "a project team changes on its project's people page",
        ));
    }
    let removed = groups::remove_members(&mut tx, id, &ids).await?;
    if !removed.is_empty() {
        keep_an_administrator(&mut tx).await?;
        groups::touch(&mut tx, &[id]).await?;
        users::touch(&mut tx, &removed).await?;
        audit::record(
            &mut tx,
            actor_of(actor),
            "group.members_removed",
            &format!("group:{id}"),
            json!({ "users": removed }),
        )
        .await?;
        changed(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(removed)
}

/// What deleting groups takes away, shown before it happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeletionImpact {
    pub groups: i64,
    pub memberships: i64,
    pub assignments: i64,
}

pub async fn deletion_impact(
    pool: &PgPool,
    actor: &Access,
    ids: &[GroupId],
) -> Result<DeletionImpact> {
    actor.may_manage_groups()?;
    let ids = bulk(ids, "groups")?;
    let mut conn = pool.acquire().await?;
    let found = groups::get_many(&mut *conn, &ids).await?;
    let counts = groups::member_counts(&mut *conn, &ids).await?;
    let assignments = assignments::of_groups(&mut *conn, &ids).await?;
    Ok(DeletionImpact {
        groups: i64::try_from(found.len()).unwrap_or(i64::MAX),
        memberships: counts.iter().map(|(_, count)| count).sum(),
        assignments: i64::try_from(assignments.len()).unwrap_or(i64::MAX),
    })
}

/// Deletes groups (ACC-032): their roles are revoked, members leave, and their IDs resolve to the
/// replacement, which also takes over every reference to them: places in project teams and user
/// value sets, projects' default audiences, issue audiences and group fields, so
/// restricted content never becomes visible to everyone. Project teams go only with their
/// project.
pub async fn delete(
    pool: &PgPool,
    actor: &Access,
    ids: &[GroupId],
    replacement: GroupId,
) -> Result<()> {
    actor.may_manage_groups()?;
    let ids = bulk(ids, "groups")?;
    if ids.contains(&replacement) {
        return Err(Error::field(
            "replacement",
            Problem::Invalid,
            "the replacement cannot be one of the deleted groups",
        ));
    }
    let mut every = ids.clone();
    every.push(replacement);
    let mut tx = pool.begin().await?;
    let found = groups::lock_many(&mut tx, &every).await?;
    if found.len() != every.len() {
        return Err(Error::NotFound("no such group"));
    }
    if found
        .iter()
        .any(|group| group.builtin.is_some() && group.id != replacement)
    {
        return Err(Error::Conflict("built-in groups cannot be deleted"));
    }
    if found.iter().any(Group::is_team) {
        return Err(Error::Conflict(
            "a project team is deleted with its project and replaces no group",
        ));
    }
    // What referred to the deleted groups now refers to the replacement (architecture.md §6).
    teams::replace_groups(&mut tx, &ids, replacement).await?;
    value_sets::replace_groups(&mut tx, &ids, replacement).await?;
    projects::replace_audience(&mut tx, &ids, replacement).await?;
    trackflow_store::issues::replace_audience_groups(&mut tx, &ids, replacement).await?;
    trackflow_store::comments::replace_audience_groups(&mut tx, &ids, replacement).await?;
    trackflow_store::attachments::replace_audience_groups(&mut tx, &ids, replacement).await?;
    trackflow_store::issue_values::replace_groups(&mut tx, &ids, replacement).await?;
    trackflow_store::tags::replace_audience_groups(&mut tx, &ids, replacement).await?;
    trackflow_store::saved_searches::replace_audience_groups(&mut tx, &ids, replacement).await?;
    trackflow_store::boards::replace_audience_groups(&mut tx, &ids, replacement).await?;
    groups::delete(&mut tx, &ids, replacement).await?;
    keep_an_administrator(&mut tx).await?;
    for group in found.iter().filter(|group| group.id != replacement) {
        audit::record(
            &mut tx,
            actor_of(actor),
            "group.deleted",
            &format!("group:{}", group.id),
            json!({ "name": group.name, "replacement": replacement }),
        )
        .await?;
    }
    changed(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// A team is visible only to people who may see its project (PRJ-009); other groups to all who
/// may see groups.
pub(crate) fn team_visible(viewer: &Access, group: &Group) -> bool {
    group
        .team_of
        .is_none_or(|project| viewer.has_in(Permission::ProjectBasic, project))
}

fn bulk(ids: &[i64], path: &str) -> Result<Vec<i64>> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(Error::field(path, Problem::Required, "choose at least one"));
    }
    if ids.len() > BULK_MAX {
        return Err(Error::field(
            path,
            Problem::TooLong,
            format!("choose at most {BULK_MAX}"),
        ));
    }
    Ok(ids)
}

fn name_taken(error: sqlx::Error) -> Error {
    if unique_violation(&error) == Some(groups::NAME_KEY) {
        Error::field("name", Problem::Taken, "another group has this name")
    } else {
        error.into()
    }
}

fn actor_of(access: &Access) -> trackflow_domain::Actor {
    trackflow_domain::Actor::User(access.user())
}
