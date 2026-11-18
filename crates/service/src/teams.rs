//! People of a project (PRJ-046…PRJ-054, ACC-033): its team of accounts and groups, the roles of
//! the team, and everyone else with roles in the project.
//!
//! The team is a group that belongs to the project. Its members hold the team roles in this
//! project only; groups in the team bring all their current members. The project's assignee sets
//! hold the team itself, so whoever joins the team can be assigned at once (PRJ-054, D-027).

use serde_json::json;
use taskmaster_domain::access::permission::{Permission, PermissionSet};
use taskmaster_domain::access::role::role_permissions;
use taskmaster_domain::access::{Access, Scope};
use taskmaster_domain::{GroupId, ProjectId, UserId};
use taskmaster_store::access::{self as assignments, Assignment};
use taskmaster_store::groups::{self, Group};
use taskmaster_store::projects::Project;
use taskmaster_store::users::{self, User};
use taskmaster_store::{PgPool, audit, roles, teams};

use crate::access::{changed, grants_of_group};
use crate::projects::{actor_of, lock_visible, require, require_active, target, visible};
use crate::roles::{Described, describe};
use crate::{Error, Result};

/// Most accounts or groups one change takes.
pub const BULK_MAX: usize = 100;
const SEARCH_MAX: usize = 100;

/// The people page of a project (PRJ-046).
#[derive(Debug, Clone)]
pub struct People {
    pub project: Project,
    pub owner: Option<User>,
    pub team: Group,
    pub team_size: i64,
    /// Groups in the team.
    pub team_groups: Vec<Group>,
    /// Roles held in this project: by the team (team roles) and directly.
    pub assignments: Vec<Described>,
    /// Global roles that reach this project, shown read-only: they change on their holder's page.
    pub global: Vec<Described>,
}

async fn team_of(conn: &mut sqlx::PgConnection, project: ProjectId) -> Result<Group> {
    let id = teams::of_project(&mut *conn, project)
        .await?
        .ok_or_else(|| Error::Internal("the project has no team".to_owned()))?;
    groups::get(&mut *conn, id)
        .await?
        .ok_or_else(|| Error::Internal("the team is gone".to_owned()))
}

/// The people page. Needs `project.full` (PRJ-009).
pub async fn people(pool: &PgPool, viewer: &Access, id: ProjectId) -> Result<People> {
    let mut conn = pool.acquire().await?;
    let project = visible(&mut conn, viewer, id).await?;
    require(viewer, Permission::ProjectFull, id)?;
    let team = team_of(&mut conn, id).await?;
    let team_size = teams::sizes(&mut *conn, &[team.id])
        .await?
        .first()
        .map_or(0, |(_, size)| *size);
    let group_ids = teams::groups(&mut *conn, team.id).await?;
    let mut team_groups = groups::get_many(&mut *conn, &group_ids).await?;
    team_groups.sort_by_key(|g| group_ids.iter().position(|id| *id == g.id));
    let owner = users::get(&mut *conn, project.owner_id).await?;
    let in_project = assignments::of_project(&mut *conn, id).await?;
    let global = assignments::global(&mut *conn).await?;
    let role_ids: Vec<i64> = global.iter().map(|a| a.role_id).collect();
    let global_roles = roles::get_many(&mut *conn, &role_ids).await?;
    drop(conn);
    // Global roles reach the project only through their project permissions (ACC-045).
    let reaching: Vec<Assignment> = global
        .into_iter()
        .filter(|a| {
            global_roles.iter().any(|role| {
                role.id == a.role_id
                    && !role_permissions(role.builtin.as_deref(), &role.permissions)
                        .intersection(PermissionSet::project_scoped())
                        .is_empty()
            })
        })
        .collect();
    Ok(People {
        assignments: describe(pool, in_project).await?,
        global: describe(pool, reaching).await?,
        project,
        owner,
        team,
        team_size,
        team_groups,
    })
}

/// A page of the accounts added to the team directly, by name (PRJ-046). Needs `project.full`.
pub async fn team_users(
    pool: &PgPool,
    viewer: &Access,
    id: ProjectId,
    text: &str,
    after: Option<(String, UserId)>,
    limit: i64,
) -> Result<Vec<User>> {
    let mut conn = pool.acquire().await?;
    visible(&mut conn, viewer, id).await?;
    require(viewer, Permission::ProjectFull, id)?;
    let team = team_of(&mut conn, id).await?;
    let text: String = text.trim().chars().take(SEARCH_MAX).collect();
    let (name, user) = after.unwrap_or_default();
    let ids = groups::members(&mut conn, team.id, &text, (&name, user), limit).await?;
    let mut found = users::get_many(&mut *conn, &ids).await?;
    found.sort_by_key(|user| ids.iter().position(|id| *id == user.id));
    Ok(found)
}

/// Who joined or left a team.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changed {
    pub users: Vec<UserId>,
    pub groups: Vec<GroupId>,
}

fn bulk(ids: &[i64], path: &str) -> Result<Vec<i64>> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() > BULK_MAX {
        return Err(Error::field(
            path,
            taskmaster_domain::validation::Problem::TooLong,
            format!("choose at most {BULK_MAX}"),
        ));
    }
    Ok(ids)
}

/// Adds accounts and groups to the team (PRJ-048). Needs `project.update` and every permission
/// the team roles give there (ACC-050); groups bring all their current members, and project teams
/// cannot be members of other teams.
pub async fn add(
    pool: &PgPool,
    actor: &Access,
    id: ProjectId,
    user_ids: &[UserId],
    group_ids: &[GroupId],
) -> Result<Changed> {
    let user_ids = bulk(user_ids, "users")?;
    let group_ids = bulk(group_ids, "groups")?;
    if user_ids.is_empty() && group_ids.is_empty() {
        return Err(Error::field(
            "users",
            taskmaster_domain::validation::Problem::Required,
            "choose at least one account or group",
        ));
    }
    let mut tx = pool.begin().await?;
    let project = lock_visible(&mut tx, actor, id).await?;
    require(actor, Permission::ProjectUpdate, id)?;
    require_active(&project)?;
    let team = team_of(&mut tx, id).await?;
    let grants = grants_of_group(&mut *tx, team.id).await?;
    actor.may_join_to(&grants)?;
    let found = users::get_many(&mut *tx, &user_ids).await?;
    if found.len() != user_ids.len() || found.iter().any(User::is_deleted) {
        return Err(Error::NotFound("no such account"));
    }
    let found_groups = groups::lock_many(&mut tx, &group_ids).await?;
    if found_groups.len() != group_ids.len() {
        return Err(Error::NotFound("no such group"));
    }
    if found_groups.iter().any(Group::is_team) {
        return Err(Error::field(
            "groups",
            taskmaster_domain::validation::Problem::Invalid,
            "a project team cannot be in another team",
        ));
    }
    let users_added = groups::add_members(&mut tx, team.id, &user_ids).await?;
    let groups_added = teams::add_groups(&mut tx, team.id, &group_ids).await?;
    if !users_added.is_empty() || !groups_added.is_empty() {
        groups::touch(&mut tx, &[team.id]).await?;
        users::touch(&mut tx, &users_added).await?;
        taskmaster_store::projects::touch(&mut tx, &[id]).await?;
        audit::record(
            &mut tx,
            actor_of(actor),
            "project.team_added",
            &target(id),
            json!({ "users": users_added, "groups": groups_added }),
        )
        .await?;
        changed(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(Changed {
        users: users_added,
        groups: groups_added,
    })
}

/// Removes accounts added directly and groups from the team (PRJ-050). Members through a group
/// leave with the group. Needs `project.update`. They keep any direct roles in the project.
pub async fn remove(
    pool: &PgPool,
    actor: &Access,
    id: ProjectId,
    user_ids: &[UserId],
    group_ids: &[GroupId],
) -> Result<Changed> {
    let user_ids = bulk(user_ids, "users")?;
    let group_ids = bulk(group_ids, "groups")?;
    let mut tx = pool.begin().await?;
    let project = lock_visible(&mut tx, actor, id).await?;
    require(actor, Permission::ProjectUpdate, id)?;
    require_active(&project)?;
    let team = team_of(&mut tx, id).await?;
    let users_removed = groups::remove_members(&mut tx, team.id, &user_ids).await?;
    let groups_removed = teams::remove_groups(&mut tx, team.id, &group_ids).await?;
    if !users_removed.is_empty() || !groups_removed.is_empty() {
        groups::touch(&mut tx, &[team.id]).await?;
        users::touch(&mut tx, &users_removed).await?;
        taskmaster_store::projects::touch(&mut tx, &[id]).await?;
        audit::record(
            &mut tx,
            actor_of(actor),
            "project.team_removed",
            &target(id),
            json!({ "users": users_removed, "groups": groups_removed }),
        )
        .await?;
        changed(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(Changed {
        users: users_removed,
        groups: groups_removed,
    })
}

/// Checks a role assignment to a project team (ACC-033): a team holds roles only in its own
/// project.
pub(crate) fn check_team_scope(team: &Group, scope: Scope) -> Result<()> {
    match (team.team_of, scope) {
        (Some(project), Scope::Project(at)) if project == at => Ok(()),
        (Some(_), _) => Err(Error::field(
            "scopes",
            taskmaster_domain::validation::Problem::Invalid,
            "a project team holds roles only in its own project",
        )),
        (None, _) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;

    fn group(team_of: Option<ProjectId>) -> Group {
        Group {
            id: 1,
            name: "x".to_owned(),
            description: String::new(),
            builtin: None,
            auto_join: false,
            team_of,
            version: 1,
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn acc_033_teams_hold_roles_only_in_their_project() {
        assert!(check_team_scope(&group(Some(4)), Scope::Project(4)).is_ok());
        assert!(check_team_scope(&group(Some(4)), Scope::Project(5)).is_err());
        assert!(check_team_scope(&group(Some(4)), Scope::Global).is_err());
        assert!(check_team_scope(&group(None), Scope::Global).is_ok());
    }
}
