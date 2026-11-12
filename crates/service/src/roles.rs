//! Roles and role assignments (ACC-052…ACC-060): the list, custom roles and their permissions,
//! deleting them, and assigning roles to accounts and groups. Only `system.write` holders create
//! and edit roles (D-013); nobody gives a role a permission they do not hold (ACC-050).

use serde_json::json;
use trackflow_domain::access::permission::{Permission, PermissionSet};
use trackflow_domain::access::role::{
    PredefinedRole, ROLE_DESCRIPTION_MAX, description, role_name, role_permissions, toggle,
};
use trackflow_domain::access::{Access, AssignmentProblem, Scope, check_assignment};
use trackflow_domain::validation::{Problem, Problems};
use trackflow_domain::{ProjectId, RoleId};
use trackflow_store::access::{self as assignments, Assignment, Holder};
use trackflow_store::roles::{self, Role};
use trackflow_store::{PgPool, audit, groups, projects, unique_violation, users};

use crate::access::{changed, keep_an_administrator, scope_of};
use crate::{Error, Result};

const SEARCH_MAX: usize = 100;

/// A role with its effective permissions and how many assignments it has.
#[derive(Debug, Clone)]
pub struct RoleRow {
    pub role: Role,
    pub permissions: PermissionSet,
    pub assignments: i64,
}

impl RoleRow {
    pub fn predefined(&self) -> Option<PredefinedRole> {
        self.role.builtin.as_deref().and_then(PredefinedRole::parse)
    }
}

fn permissions_of(role: &Role) -> PermissionSet {
    role_permissions(role.builtin.as_deref(), &role.permissions)
}

/// The order of the roles list: predefined roles in catalog order, then custom ones by name.
fn sort_key(row: &RoleRow) -> (usize, String, RoleId) {
    match row.predefined() {
        Some(role) => (
            PredefinedRole::ALL
                .iter()
                .position(|known| *known == role)
                .unwrap_or(0),
            String::new(),
            row.role.id,
        ),
        None => (
            PredefinedRole::ALL.len(),
            row.role.name.to_lowercase(),
            row.role.id,
        ),
    }
}

/// Roles whose name (in any language for predefined ones) contains `text`, or that contain
/// `permission` (ACC-053). Roles are few, so the list is filtered and paged in memory; `after` is
/// the previous page's last role ID.
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    text: &str,
    permission: Option<Permission>,
    after: Option<RoleId>,
    limit: usize,
) -> Result<Vec<RoleRow>> {
    viewer.may_see_roles()?;
    let text: String = text
        .trim()
        .to_lowercase()
        .chars()
        .take(SEARCH_MAX)
        .collect();
    let counts = assignments::counts_by_role(pool).await?;
    let mut rows: Vec<RoleRow> = roles::all(pool)
        .await?
        .into_iter()
        .map(|role| {
            let permissions = permissions_of(&role);
            let assignments = counts
                .iter()
                .find(|(id, _)| *id == role.id)
                .map_or(0, |(_, count)| *count);
            RoleRow {
                role,
                permissions,
                assignments,
            }
        })
        .filter(|row| {
            let names_match = text.is_empty()
                || row.role.name.to_lowercase().contains(&text)
                || row.predefined().is_some_and(|role| {
                    role.names()
                        .iter()
                        .any(|n| n.to_lowercase().contains(&text))
                });
            names_match && permission.is_none_or(|p| row.permissions.contains(p))
        })
        .collect();
    rows.sort_by_key(sort_key);
    let start = after
        .and_then(|after| rows.iter().position(|row| row.role.id == after))
        .map_or(0, |index| index + 1);
    Ok(rows.into_iter().skip(start).take(limit).collect())
}

/// One role.
pub async fn get(pool: &PgPool, viewer: &Access, id: RoleId) -> Result<RoleRow> {
    viewer.may_see_roles()?;
    let role = roles::get(pool, id)
        .await?
        .ok_or(Error::NotFound("no such role"))?;
    let assignments = assignments::of_role(pool, id).await?;
    Ok(RoleRow {
        permissions: permissions_of(&role),
        assignments: i64::try_from(assignments.len()).unwrap_or(i64::MAX),
        role,
    })
}

/// A custom role to create (ACC-054).
#[derive(Clone, Default)]
pub struct NewRole {
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
}

/// Creates a custom role. Its permissions are closed under implication (ACC-046), and each must
/// be held by the creator (ACC-050).
pub async fn create(pool: &PgPool, actor: &Access, input: NewRole) -> Result<RoleRow> {
    actor.require(Permission::SystemWrite)?;
    let mut problems = Problems::new();
    let name = problems.check("name", role_name(&input.name));
    let description = problems.check(
        "description",
        description(&input.description, ROLE_DESCRIPTION_MAX),
    );
    let mut permissions = PermissionSet::EMPTY;
    for key in &input.permissions {
        match Permission::parse(key) {
            Some(permission) => permissions.insert(permission),
            None => problems.add(
                "permissions",
                trackflow_domain::validation::Rejection::new(
                    Problem::Invalid,
                    format!("unknown permission {key:?}"),
                ),
            ),
        }
    }
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(description)) = (name, description) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    let permissions = permissions.closure();
    actor.may_pass_on(permissions, Scope::Global)?;
    let keys = stored(permissions);
    let mut tx = pool.begin().await?;
    let id = roles::insert(&mut tx, &name, &description, &keys)
        .await
        .map_err(name_taken)?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "role.created",
        &format!("role:{id}"),
        json!({ "name": name, "permissions": keys }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Changes a custom role's name or description; `None` keeps a value.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    id: RoleId,
    name: Option<&str>,
    new_description: Option<&str>,
    version: i64,
) -> Result<RoleRow> {
    actor.require(Permission::SystemWrite)?;
    let mut problems = Problems::new();
    let name = name.and_then(|raw| problems.check("name", role_name(raw)));
    let new_description = new_description
        .and_then(|raw| problems.check("description", description(raw, ROLE_DESCRIPTION_MAX)));
    problems.finish().map_err(Error::Invalid)?;
    let mut tx = pool.begin().await?;
    let role = lock_custom(&mut tx, id).await?;
    if role.version != version {
        return Err(Error::Conflict("the role was changed by someone else"));
    }
    roles::update(&mut tx, id, name.as_deref(), new_description.as_deref())
        .await
        .map_err(name_taken)?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "role.updated",
        &format!("role:{id}"),
        json!({
            "name": name.as_ref().map(|new| json!({ "old": role.name, "new": new })),
            "description": new_description.as_ref().map(|new| json!({ "old": role.description, "new": new })),
        }),
    )
    .await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Switches one permission of a custom role on or off (ACC-046, ACC-054): on adds what it implies
/// (each must be held by the actor, ACC-050), off removes what depends on it.
pub async fn set_permission(
    pool: &PgPool,
    actor: &Access,
    id: RoleId,
    permission: Permission,
    on: bool,
) -> Result<RoleRow> {
    actor.require(Permission::SystemWrite)?;
    let mut tx = pool.begin().await?;
    let role = lock_custom(&mut tx, id).await?;
    let before = permissions_of(&role);
    let after = toggle(before, permission, on);
    if after == before {
        tx.rollback().await?;
        return get(pool, actor, id).await;
    }
    actor.may_pass_on(after.difference(before), Scope::Global)?;
    roles::set_permissions(&mut tx, id, &stored(after)).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "role.permissions_changed",
        &format!("role:{id}"),
        json!({
            "added": after.difference(before).keys(),
            "removed": before.difference(after).keys(),
        }),
    )
    .await?;
    changed(&mut tx).await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Deletes a custom role and revokes all its assignments (ACC-057). Predefined roles stay.
pub async fn delete(pool: &PgPool, actor: &Access, id: RoleId) -> Result<()> {
    actor.require(Permission::SystemWrite)?;
    let mut tx = pool.begin().await?;
    let role = lock_custom(&mut tx, id).await?;
    let revoked = assignments::of_role(&mut *tx, id).await?;
    roles::delete(&mut tx, id).await?;
    keep_an_administrator(&mut tx).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "role.deleted",
        &format!("role:{id}"),
        json!({ "name": role.name, "assignments_revoked": revoked.len() }),
    )
    .await?;
    changed(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Assignments of a role, for showing what deleting it revokes (ACC-057).
pub async fn assignments_of(pool: &PgPool, viewer: &Access, id: RoleId) -> Result<Vec<Assignment>> {
    viewer.may_see_roles()?;
    roles::get(pool, id)
        .await?
        .ok_or(Error::NotFound("no such role"))?;
    Ok(assignments::of_role(pool, id).await?)
}

/// The direct assignments of an account or a group (ACC-059, ACC-060): visible to the owner and
/// to whoever may see roles.
pub async fn assignments_of_holder(
    pool: &PgPool,
    viewer: &Access,
    holder: Holder,
) -> Result<Vec<Assignment>> {
    if holder != Holder::User(viewer.user()) {
        viewer.may_see_roles()?;
    }
    Ok(assignments::of_holder(pool, holder).await?)
}

/// Assigns a role to an account or a group at one or more scopes (ACC-058…ACC-060). Each scope
/// needs the right to assign there (ACC-051), a role that means something there (ACC-045) and
/// every permission the role gives there (ACC-050). Existing assignments are kept as they are.
pub async fn assign(
    pool: &PgPool,
    actor: &Access,
    role: RoleId,
    holder: Holder,
    scopes: &[Option<ProjectId>],
) -> Result<Vec<Assignment>> {
    let mut scopes = scopes.to_vec();
    scopes.sort_unstable();
    scopes.dedup();
    if scopes.is_empty() {
        return Err(Error::field(
            "scopes",
            Problem::Required,
            "choose where the role applies",
        ));
    }
    let mut tx = pool.begin().await?;
    let project_ids: Vec<ProjectId> = scopes.iter().flatten().copied().collect();
    let found_projects = projects::lock_many(&mut tx, &project_ids).await?;
    if found_projects.len() != project_ids.len()
        || found_projects
            .iter()
            .any(|project| !crate::projects::can_see(actor, project))
    {
        return Err(Error::field("scopes", Problem::Invalid, "no such project"));
    }
    let found = roles::get(&mut *tx, role)
        .await?
        .ok_or_else(|| Error::field("role", Problem::Invalid, "no such role"))?;
    let permissions = permissions_of(&found);
    let team = match holder {
        Holder::Group(id) => groups::get(&mut *tx, id)
            .await?
            .filter(groups::Group::is_team),
        Holder::User(_) => None,
    };
    for project in &scopes {
        let scope = scope_of(*project);
        if let Some(team) = &team {
            crate::teams::check_team_scope(team, scope)?;
        }
        actor.may_assign_at(scope)?;
        check_assignment(permissions, scope).map_err(|problem| match problem {
            AssignmentProblem::NoProjectPermissions => Error::field(
                "scopes",
                Problem::Invalid,
                "the role has no project permissions",
            ),
        })?;
        actor.may_pass_on(permissions, scope)?;
    }
    match holder {
        Holder::User(id) => {
            let user = users::lock(&mut tx, id)
                .await?
                .filter(|user| !user.is_deleted())
                .ok_or(Error::NotFound("no such account"))?;
            users::touch(&mut tx, &[user.id]).await?;
        }
        Holder::Group(id) => {
            let group = groups::lock_many(&mut tx, &[id])
                .await?
                .pop()
                .ok_or(Error::NotFound("no such group"))?;
            groups::touch(&mut tx, &[group.id]).await?;
        }
    }
    let mut created = Vec::new();
    for project in &scopes {
        if let Some(id) =
            assignments::insert(&mut tx, role, holder, *project, Some(actor.user())).await?
        {
            created.push(id);
        }
    }
    if !created.is_empty() {
        projects::touch(&mut tx, &project_ids).await?;
        audit::record(
            &mut tx,
            actor_of(actor),
            "role.assigned",
            &holder_target(holder),
            json!({ "role": role, "assignments": created, "projects": scopes }),
        )
        .await?;
        changed(&mut tx).await?;
    }
    tx.commit().await?;
    let all = assignments::of_holder(pool, holder).await?;
    Ok(all
        .into_iter()
        .filter(|a| created.contains(&a.id))
        .collect())
}

/// Revokes a direct assignment (ACC-059, ACC-060). Roles through a group end by leaving it.
pub async fn revoke(pool: &PgPool, actor: &Access, assignment: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    let found = assignments::lock(&mut tx, assignment)
        .await?
        .ok_or(Error::NotFound("no such assignment"))?;
    actor.may_assign_at(scope_of(found.project_id))?;
    assignments::delete(&mut tx, assignment).await?;
    if let Some(project) = found.project_id {
        projects::touch(&mut tx, &[project]).await?;
    }
    keep_an_administrator(&mut tx).await?;
    match found.holder {
        Holder::User(id) => users::touch(&mut tx, &[id]).await?,
        Holder::Group(id) => groups::touch(&mut tx, &[id]).await?,
    }
    audit::record(
        &mut tx,
        actor_of(actor),
        "role.revoked",
        &holder_target(found.holder),
        json!({ "role": found.role_id, "assignment": assignment, "project": found.project_id }),
    )
    .await?;
    changed(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// An assignment with its role and holder, for showing it.
#[derive(Debug, Clone)]
pub struct Described {
    pub assignment: Assignment,
    pub role: Role,
    pub user: Option<users::User>,
    pub group: Option<groups::Group>,
}

/// Loads the roles and holders of assignments: one query per kind.
pub async fn describe(pool: &PgPool, list: Vec<Assignment>) -> Result<Vec<Described>> {
    let mut conn = pool.acquire().await?;
    let role_ids: Vec<RoleId> = list.iter().map(|a| a.role_id).collect();
    let user_ids: Vec<i64> = list
        .iter()
        .filter_map(|a| match a.holder {
            Holder::User(id) => Some(id),
            Holder::Group(_) => None,
        })
        .collect();
    let group_ids: Vec<i64> = list
        .iter()
        .filter_map(|a| match a.holder {
            Holder::Group(id) => Some(id),
            Holder::User(_) => None,
        })
        .collect();
    let found_roles = roles::get_many(&mut *conn, &role_ids).await?;
    let found_users = users::get_many(&mut *conn, &user_ids).await?;
    let found_groups = groups::get_many(&mut *conn, &group_ids).await?;
    Ok(list
        .into_iter()
        .filter_map(|assignment| {
            let role = found_roles
                .iter()
                .find(|r| r.id == assignment.role_id)?
                .clone();
            let (user, group) = match assignment.holder {
                Holder::User(id) => (Some(found_users.iter().find(|u| u.id == id)?.clone()), None),
                Holder::Group(id) => (
                    None,
                    Some(found_groups.iter().find(|g| g.id == id)?.clone()),
                ),
            };
            Some(Described {
                assignment,
                role,
                user,
                group,
            })
        })
        .collect())
}

fn holder_target(holder: Holder) -> String {
    match holder {
        Holder::User(id) => format!("user:{id}"),
        Holder::Group(id) => format!("group:{id}"),
    }
}

async fn lock_custom(conn: &mut sqlx::PgConnection, id: RoleId) -> Result<Role> {
    let role = roles::lock(conn, id)
        .await?
        .ok_or(Error::NotFound("no such role"))?;
    if role.builtin.is_some() {
        return Err(Error::Conflict("predefined roles cannot change"));
    }
    Ok(role)
}

fn stored(permissions: PermissionSet) -> Vec<String> {
    permissions.keys().into_iter().map(str::to_owned).collect()
}

fn name_taken(error: sqlx::Error) -> Error {
    if unique_violation(&error) == Some(roles::NAME_KEY) {
        Error::field("name", Problem::Taken, "another role has this name")
    } else {
        error.into()
    }
}

fn actor_of(access: &Access) -> trackflow_domain::Actor {
    trackflow_domain::Actor::User(access.user())
}
