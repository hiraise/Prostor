//! A person's effective access (ACC-058, ACC-061, ACC-062): compiled from their assignments,
//! their groups' assignments and the capabilities of their groups. The server caches the result
//! per person and drops it when [`changed`] is signalled from any node.

use std::collections::BTreeMap;

use sqlx::{PgConnection, PgExecutor};
use taskmaster_domain::access::capability::{Capability, CapabilitySet};
use taskmaster_domain::access::group::BuiltinGroup;
use taskmaster_domain::access::role::role_permissions;
use taskmaster_domain::access::{Access, Grant, Scope};
use taskmaster_domain::{GroupId, ProjectId, RoleId, UserId};
use taskmaster_store::access::{self as assignments, GrantRow, Holder};
use taskmaster_store::users::User;
use taskmaster_store::{PgPool, bus, groups, roles, users};

use crate::{Error, Result};

/// Compiles the access of an account. A deleted or disabled account has none.
pub async fn load(pool: &PgPool, user: UserId) -> Result<Access> {
    let mut conn = pool.acquire().await?;
    let account = users::get(&mut *conn, user)
        .await?
        .ok_or(Error::NotFound("the account no longer exists"))?;
    compile(&mut conn, &account).await
}

/// Compiles the access of a loaded account inside the caller's connection or transaction.
pub async fn compile(conn: &mut PgConnection, account: &User) -> Result<Access> {
    if account.is_deleted() || account.disabled_at.is_some() {
        return Ok(Access::none(account.id));
    }
    let groups = groups::of_user(&mut *conn, account.id, account.is_guest()).await?;
    let rows = assignments::grants(&mut *conn, account.id, &groups).await?;
    let capabilities = assignments::capabilities_of(&mut *conn, &groups).await?;
    Ok(Access::compile(
        account.id,
        rows.iter().map(grant),
        groups,
        capabilities
            .iter()
            .filter_map(|key| Capability::parse(key))
            .collect::<CapabilitySet>(),
    ))
}

/// Compiles the access of several loaded accounts in three queries (mention candidates, CNT-054).
pub async fn compile_many(conn: &mut PgConnection, accounts: &[User]) -> Result<Vec<Access>> {
    let ids: Vec<UserId> = accounts.iter().map(|a| a.id).collect();
    let memberships = groups::of_users(&mut *conn, &ids).await?;
    let mut all_groups: Vec<GroupId> = memberships.iter().map(|(_, g)| *g).collect();
    all_groups.sort_unstable();
    all_groups.dedup();
    let rows = assignments::grants_of_users(&mut *conn, &ids, &all_groups).await?;
    let capabilities = assignments::capabilities_of_groups(&mut *conn, &all_groups).await?;
    Ok(accounts
        .iter()
        .map(|account| {
            if account.is_deleted() || account.disabled_at.is_some() {
                return Access::none(account.id);
            }
            let mine: Vec<GroupId> = memberships
                .iter()
                .filter(|(user, _)| *user == account.id)
                .map(|(_, group)| *group)
                .collect();
            let grants = rows.iter().filter(|row| match row.assignment.holder {
                Holder::User(user) => user == account.id,
                Holder::Group(group) => mine.contains(&group),
            });
            let caps: CapabilitySet = capabilities
                .iter()
                .filter(|(group, _)| mine.contains(group))
                .filter_map(|(_, key)| Capability::parse(key))
                .collect();
            Access::compile(account.id, grants.map(grant), mine.clone(), caps)
        })
        .collect())
}

/// The grant one assignment gives.
fn grant(row: &GrantRow) -> Grant {
    Grant {
        permissions: role_permissions(row.role_builtin.as_deref(), &row.role_permissions),
        scope: scope_of(row.assignment.project_id),
    }
}

pub fn scope_of(project: Option<ProjectId>) -> Scope {
    match project {
        None => Scope::Global,
        Some(project) => Scope::Project(project),
    }
}

/// Tells every node that someone's access may have changed; part of the caller's transaction.
pub async fn changed(conn: &mut PgConnection) -> Result<()> {
    bus::notify(conn, bus::ACCESS, "").await?;
    Ok(())
}

/// Refuses a change that would leave nobody able to administer the instance: some enabled account
/// other than the guest must still hold the system administrator role globally. Call inside the
/// change's transaction, after the change.
pub async fn keep_an_administrator(conn: &mut PgConnection) -> Result<()> {
    if assignments::administrator_remains(conn).await? {
        Ok(())
    } else {
        Err(Error::LastAdministrator)
    }
}

/// Where a person's role comes from (ACC-061).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Assigned to the person; revocable from their profile.
    Direct { assignment: i64 },
    /// Through a group, a built-in one or a project team included.
    Group {
        id: GroupId,
        name: String,
        builtin: Option<BuiltinGroup>,
        /// The project whose team it is (ACC-033: "via team").
        team_of: Option<ProjectId>,
    },
}

/// One role a person holds at one scope, with every source of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveRole {
    pub role: roles::Role,
    pub scope: Scope,
    pub sources: Vec<Source>,
}

/// Every role an account holds, per scope, with its sources (ACC-018, ACC-061). Global first,
/// then by role order (predefined first, then by name).
pub async fn effective_roles(db: &PgPool, account: &User) -> Result<Vec<EffectiveRole>> {
    let mut conn = db.acquire().await?;
    let group_ids = groups::of_user(&mut *conn, account.id, account.is_guest()).await?;
    let rows = assignments::grants(&mut *conn, account.id, &group_ids).await?;
    let group_rows = groups::get_many(&mut *conn, &group_ids).await?;
    let role_ids: Vec<RoleId> = rows.iter().map(|row| row.assignment.role_id).collect();
    let role_rows = roles::get_many(&mut *conn, &role_ids).await?;

    let mut found: BTreeMap<(Option<ProjectId>, RoleId), Vec<Source>> = BTreeMap::new();
    for row in &rows {
        let source = match row.assignment.holder {
            Holder::User(_) => Source::Direct {
                assignment: row.assignment.id,
            },
            Holder::Group(id) => {
                let Some(group) = group_rows.iter().find(|group| group.id == id) else {
                    continue;
                };
                Source::Group {
                    id,
                    name: group.name.clone(),
                    builtin: group.builtin.as_deref().and_then(BuiltinGroup::parse),
                    team_of: group.team_of,
                }
            }
        };
        found
            .entry((row.assignment.project_id, row.assignment.role_id))
            .or_default()
            .push(source);
    }
    let mut result: Vec<EffectiveRole> = found
        .into_iter()
        .filter_map(|((project, role), sources)| {
            let role = role_rows.iter().find(|r| r.id == role)?.clone();
            Some(EffectiveRole {
                role,
                scope: scope_of(project),
                sources,
            })
        })
        .collect();
    result.sort_by(|a, b| {
        (
            a.scope.project(),
            a.role.builtin.is_none(),
            a.role.name.to_lowercase(),
        )
            .cmp(&(
                b.scope.project(),
                b.role.builtin.is_none(),
                b.role.name.to_lowercase(),
            ))
    });
    Ok(result)
}

/// The grants a group gives its members, for the escalation check of adding members (ACC-050).
pub async fn grants_of_group(db: impl PgExecutor<'_>, group: GroupId) -> Result<Vec<Grant>> {
    let rows = assignments::grants_of_groups(db, &[group]).await?;
    Ok(rows.iter().map(grant).collect())
}
