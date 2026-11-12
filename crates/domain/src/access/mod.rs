//! Access (ACC): permissions, roles, groups, scopes, a person's effective access, capabilities,
//! what profiles reveal and who sees restricted items. Design: decisions.md D-025.
//!
//! A person's access is the union of the roles assigned to them, to each of their groups (the
//! built-in ones included) and later to their project teams, at global scope and at each
//! project's scope; there are no deny rules (ACC-058). [`Access`] is that union, compiled once per
//! person and cached by the server until anything about access changes.

pub mod capability;
pub mod group;
pub mod identity;
pub mod permission;
pub mod role;
pub mod visibility;

use std::collections::BTreeMap;

use capability::{Capability, CapabilitySet};
use permission::{Permission, PermissionSet};

use crate::{GroupId, ProjectId, UserId};

/// Where a role assignment applies (ACC-044). Organizations come with PRJ-057…PRJ-059 (P2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// Everywhere: global permissions, and project permissions in every project.
    Global,
    Project(ProjectId),
}

impl Scope {
    /// The permissions of a role that take effect when it is assigned here (ACC-045): all of them
    /// globally; only project permissions in a project.
    pub fn applied(self, role: PermissionSet) -> PermissionSet {
        match self {
            Scope::Global => role,
            Scope::Project(_) => role.intersection(PermissionSet::project_scoped()),
        }
    }

    pub fn project(self) -> Option<ProjectId> {
        match self {
            Scope::Global => None,
            Scope::Project(id) => Some(id),
        }
    }
}

/// One role a person holds at one scope, whatever the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub permissions: PermissionSet,
    pub scope: Scope,
}

/// Permissions someone lacks for what they tried; refusals name them (ACC-050).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denied {
    pub missing: PermissionSet,
}

impl Denied {
    pub fn of(permission: Permission) -> Self {
        Self {
            missing: PermissionSet::of(permission),
        }
    }
}

/// The projects where someone holds a permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Projects {
    /// Every project, present and future (granted globally).
    All,
    /// These projects only; empty means none.
    Only(Vec<ProjectId>),
}

/// A person's effective access (ACC-058, ACC-062).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    user: UserId,
    global: PermissionSet,
    /// Project permissions beyond the global ones, per project.
    projects: BTreeMap<ProjectId, PermissionSet>,
    /// Every group the person belongs to, the built-in ones included; sorted.
    groups: Vec<GroupId>,
    capabilities: CapabilitySet,
}

impl Access {
    /// Compiles the grants of one person. Each grant's permissions are first narrowed to what
    /// works at its scope (ACC-045).
    pub fn compile(
        user: UserId,
        grants: impl IntoIterator<Item = Grant>,
        mut groups: Vec<GroupId>,
        capabilities: CapabilitySet,
    ) -> Self {
        let mut global = PermissionSet::EMPTY;
        let mut projects: BTreeMap<ProjectId, PermissionSet> = BTreeMap::new();
        for grant in grants {
            let applied = grant.scope.applied(grant.permissions.closure());
            match grant.scope {
                Scope::Global => global = global.union(applied),
                Scope::Project(project) => {
                    let entry = projects.entry(project).or_default();
                    *entry = entry.union(applied);
                }
            }
        }
        projects.retain(|_, set| {
            *set = set.difference(global);
            !set.is_empty()
        });
        groups.sort_unstable();
        groups.dedup();
        Self {
            user,
            global,
            projects,
            groups,
            capabilities,
        }
    }

    /// Access of nobody: no permissions, no groups.
    pub fn none(user: UserId) -> Self {
        Self::compile(user, [], Vec::new(), CapabilitySet::default())
    }

    pub fn user(&self) -> UserId {
        self.user
    }

    /// Holds the permission globally (for project permissions: in every project).
    pub fn has(&self, permission: Permission) -> bool {
        self.global.contains(permission)
    }

    /// Holds the permission in the project, globally or there.
    pub fn has_in(&self, permission: Permission, project: ProjectId) -> bool {
        self.at(Scope::Project(project)).contains(permission)
    }

    /// Holds the permission globally or in at least one project.
    pub fn has_anywhere(&self, permission: Permission) -> bool {
        self.has(permission) || self.projects.values().any(|set| set.contains(permission))
    }

    /// Everything that works at the scope.
    pub fn at(&self, scope: Scope) -> PermissionSet {
        match scope {
            Scope::Global => self.global,
            Scope::Project(project) => {
                let extra = self.projects.get(&project).copied().unwrap_or_default();
                Scope::Project(project).applied(self.global).union(extra)
            }
        }
    }

    pub fn global(&self) -> PermissionSet {
        self.global
    }

    /// Project permissions beyond the global ones, per project.
    pub fn projects(&self) -> impl Iterator<Item = (ProjectId, PermissionSet)> + '_ {
        self.projects.iter().map(|(id, set)| (*id, *set))
    }

    /// Where the permission is held: the filter every list of project content applies.
    pub fn projects_with(&self, permission: Permission) -> Projects {
        if self.has(permission) {
            return Projects::All;
        }
        Projects::Only(
            self.projects
                .iter()
                .filter(|(_, set)| set.contains(permission))
                .map(|(id, _)| *id)
                .collect(),
        )
    }

    pub fn groups(&self) -> &[GroupId] {
        &self.groups
    }

    pub fn in_group(&self, group: GroupId) -> bool {
        self.groups.binary_search(&group).is_ok()
    }

    pub fn capability(&self, capability: Capability) -> bool {
        self.capabilities.contains(capability)
    }

    pub fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }

    /// Refuses unless the permission is held globally.
    pub fn require(&self, permission: Permission) -> Result<(), Denied> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(Denied::of(permission))
        }
    }

    /// Refuses unless the permission is held at the scope.
    pub fn require_at(&self, permission: Permission, scope: Scope) -> Result<(), Denied> {
        if self.at(scope).contains(permission) {
            Ok(())
        } else {
            Err(Denied::of(permission))
        }
    }

    /// Whether this person may pass on these permissions at the scope: put them into a role or
    /// assign a role that gives them (ACC-050). Only what they hold there themselves.
    pub fn may_pass_on(&self, permissions: PermissionSet, scope: Scope) -> Result<(), Denied> {
        let missing = scope
            .applied(permissions.closure())
            .difference(self.at(scope));
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Denied { missing })
        }
    }

    /// Whether this person may assign and revoke roles at the scope (ACC-051): `system.write`
    /// globally, `project.update` in a project.
    pub fn may_assign_at(&self, scope: Scope) -> Result<(), Denied> {
        match scope {
            Scope::Global => self.require(Permission::SystemWrite),
            Scope::Project(_) if self.has(Permission::SystemWrite) => Ok(()),
            Scope::Project(_) => self.require_at(Permission::ProjectUpdate, scope),
        }
    }

    /// Whether this person may create, change and delete groups and their members (ACC-051):
    /// `project.update` in any project or `system.write`.
    pub fn may_manage_groups(&self) -> Result<(), Denied> {
        if self.has(Permission::SystemWrite) || self.has_anywhere(Permission::ProjectUpdate) {
            Ok(())
        } else {
            Err(Denied::of(Permission::SystemWrite))
        }
    }

    /// Whether this person may see groups and their roles (ACC-022, ACC-051).
    pub fn may_see_groups(&self) -> Result<(), Denied> {
        if self.has(Permission::SystemRead) || self.has_anywhere(Permission::ProjectUpdate) {
            Ok(())
        } else {
            Err(Denied::of(Permission::SystemRead))
        }
    }

    /// Whether this person may see roles and their permissions (ACC-051, ACC-053).
    pub fn may_see_roles(&self) -> Result<(), Denied> {
        if self.has(Permission::SystemRead) || self.has_anywhere(Permission::ProjectFull) {
            Ok(())
        } else {
            Err(Denied::of(Permission::SystemRead))
        }
    }

    /// Whether this person may add accounts to a group with these grants (ACC-026, ACC-050):
    /// they must hold every permission the group gives, unless they hold `system.write`.
    pub fn may_join_to(&self, grants: &[Grant]) -> Result<(), Denied> {
        if self.has(Permission::SystemWrite) {
            return Ok(());
        }
        let mut missing = PermissionSet::EMPTY;
        for grant in grants {
            if let Err(denied) = self.may_pass_on(grant.permissions, grant.scope) {
                missing = missing.union(denied.missing);
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Denied { missing })
        }
    }
}

/// Why a role cannot be assigned at a scope (ACC-045).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentProblem {
    /// Nothing of the role works in a project.
    NoProjectPermissions,
}

/// Checks that a role means something at the scope: a project assignment needs at least one
/// project permission in the role (ACC-045).
pub fn check_assignment(role: PermissionSet, scope: Scope) -> Result<(), AssignmentProblem> {
    match scope {
        Scope::Project(_) if scope.applied(role).is_empty() => {
            Err(AssignmentProblem::NoProjectPermissions)
        }
        _ => Ok(()),
    }
}

/// The permissions of a role that an assignment at the scope ignores; the UI warns about them.
pub fn ignored_at(role: PermissionSet, scope: Scope) -> PermissionSet {
    role.difference(scope.applied(role))
}

#[cfg(test)]
mod tests {
    use super::role::PredefinedRole;
    use super::*;

    fn grant(role: PredefinedRole, scope: Scope) -> Grant {
        Grant {
            permissions: role.permissions(),
            scope,
        }
    }

    #[test]
    fn acc_058_access_is_the_union_of_grants() {
        let access = Access::compile(
            7,
            [
                grant(PredefinedRole::DirectoryViewer, Scope::Global),
                grant(PredefinedRole::ProjectMember, Scope::Project(1)),
                grant(PredefinedRole::ProjectAdmin, Scope::Project(2)),
            ],
            vec![5, 3, 5],
            CapabilitySet::default(),
        );
        assert_eq!(access.user(), 7);
        assert!(access.has(Permission::UserDetails));
        assert!(!access.has(Permission::IssueRead));
        assert!(access.has_in(Permission::IssueRead, 1));
        assert!(access.has_in(Permission::IssueRead, 2));
        assert!(!access.has_in(Permission::IssueRead, 3));
        assert!(access.has_in(Permission::ProjectUpdate, 2));
        assert!(!access.has_in(Permission::ProjectUpdate, 1));
        assert!(access.has_anywhere(Permission::ProjectUpdate));
        assert_eq!(
            access.projects_with(Permission::IssueRead),
            Projects::Only(vec![1, 2])
        );
        assert_eq!(access.projects_with(Permission::UserDetails), Projects::All);
        assert_eq!(access.groups(), [3, 5]);
        assert!(access.in_group(5) && !access.in_group(4));
    }

    #[test]
    fn acc_051_rights_to_manage_access() {
        let reader = Access::compile(
            1,
            [Grant {
                permissions: PermissionSet::of(Permission::SystemRead),
                scope: Scope::Global,
            }],
            vec![],
            CapabilitySet::default(),
        );
        // system.read sees groups and roles but changes nothing.
        assert!(reader.may_see_groups().is_ok() && reader.may_see_roles().is_ok());
        assert!(reader.may_manage_groups().is_err());
        assert!(reader.may_assign_at(Scope::Global).is_err());
        // project.full in a project shows roles; project.update anywhere manages groups and
        // assigns roles in that project only.
        let full = Access::compile(
            2,
            [Grant {
                permissions: PermissionSet::of(Permission::ProjectFull),
                scope: Scope::Project(3),
            }],
            vec![],
            CapabilitySet::default(),
        );
        assert!(full.may_see_roles().is_ok() && full.may_see_groups().is_err());
        let admin = Access::compile(
            3,
            [grant(PredefinedRole::ProjectAdmin, Scope::Project(3))],
            vec![],
            CapabilitySet::default(),
        );
        assert!(admin.may_manage_groups().is_ok());
        assert!(admin.may_assign_at(Scope::Project(3)).is_ok());
        assert!(admin.may_assign_at(Scope::Project(4)).is_err());
        assert!(admin.may_assign_at(Scope::Global).is_err());
    }

    #[test]
    fn acc_044_global_grants_reach_every_project() {
        let access = Access::compile(
            1,
            [
                grant(PredefinedRole::ProjectMember, Scope::Global),
                grant(PredefinedRole::ProjectMember, Scope::Project(9)),
            ],
            vec![],
            CapabilitySet::default(),
        );
        assert!(access.has_in(Permission::IssueRead, 42));
        // The project grant adds nothing beyond the global one, so it is not kept.
        assert_eq!(access.projects().count(), 0);
    }

    #[test]
    fn acc_045_global_permissions_in_a_project_role_are_ignored() {
        let access = Access::compile(
            1,
            [grant(PredefinedRole::SystemAdmin, Scope::Project(4))],
            vec![],
            CapabilitySet::default(),
        );
        assert!(access.has_in(Permission::ProjectDelete, 4));
        assert!(!access.has(Permission::UserDirectory));
        assert!(!access.has_in(Permission::UserDirectory, 4));
        assert_eq!(
            check_assignment(
                PredefinedRole::DirectoryViewer.permissions(),
                Scope::Project(4)
            ),
            Err(AssignmentProblem::NoProjectPermissions)
        );
        assert!(
            check_assignment(PredefinedRole::DirectoryViewer.permissions(), Scope::Global).is_ok()
        );
        assert_eq!(
            ignored_at(PredefinedRole::SystemAdmin.permissions(), Scope::Project(4)).len(),
            9
        );
    }

    #[test]
    fn acc_050_nobody_passes_on_what_they_do_not_hold() {
        let project_admin = Access::compile(
            1,
            [grant(PredefinedRole::ProjectAdmin, Scope::Project(4))],
            vec![],
            CapabilitySet::default(),
        );
        // A project administrator may hand out project member in their project…
        assert!(
            project_admin
                .may_pass_on(
                    PredefinedRole::ProjectMember.permissions(),
                    Scope::Project(4)
                )
                .is_ok()
        );
        // …not elsewhere, and not what they lack themselves.
        assert!(
            project_admin
                .may_pass_on(
                    PredefinedRole::ProjectMember.permissions(),
                    Scope::Project(5)
                )
                .is_err()
        );
        let denied = project_admin
            .may_pass_on(PredefinedRole::SystemAdmin.permissions(), Scope::Project(4))
            .unwrap_err();
        assert_eq!(
            denied.missing.keys(),
            ["project.delete", "visibility.bypass"]
        );
        assert!(project_admin.may_assign_at(Scope::Project(4)).is_ok());
        assert!(project_admin.may_assign_at(Scope::Global).is_err());
        assert!(project_admin.may_manage_groups().is_ok());

        let system_admin = Access::compile(
            2,
            [grant(PredefinedRole::SystemAdmin, Scope::Global)],
            vec![],
            CapabilitySet::default(),
        );
        assert!(
            system_admin
                .may_join_to(&[grant(PredefinedRole::SystemAdmin, Scope::Global)])
                .is_ok()
        );
        // Even system.write does not put into a role what its holder lacks.
        let operator = Access::compile(
            4,
            [Grant {
                permissions: PermissionSet::of(Permission::SystemWrite),
                scope: Scope::Global,
            }],
            vec![],
            CapabilitySet::default(),
        );
        assert_eq!(
            operator
                .may_pass_on(PermissionSet::of(Permission::UserDelete), Scope::Global)
                .unwrap_err()
                .missing
                .keys(),
            ["user.directory", "user.details", "user.delete"]
        );
        assert!(
            operator
                .may_join_to(&[grant(PredefinedRole::AccountCreator, Scope::Global)])
                .is_ok()
        );
        let member = Access::none(3);
        assert_eq!(
            member
                .may_join_to(&[grant(PredefinedRole::AccountCreator, Scope::Global)])
                .unwrap_err()
                .missing
                .keys(),
            ["user.create"]
        );
        assert!(member.may_join_to(&[]).is_ok());
        assert!(member.may_manage_groups().is_err());
        assert!(member.may_see_roles().is_err());
    }
}
