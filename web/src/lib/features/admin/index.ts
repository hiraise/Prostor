// The administration area (ADM-001): Access pages for users, groups and roles (ACC).
export { default as AdminNav } from './AdminNav.svelte';
export * as adminMessages from './messages.ts';
// Roles and permissions (ACC-034…ACC-057).
export { default as RolePage } from './roles/RolePage.svelte';
export { default as RolesPage } from './roles/RolesPage.svelte';
export {
  ensureRole,
  ensureRoleAssignments,
  ensureRoles,
  permissionCatalog,
  roleFilters,
  type RoleFilters,
} from './roles/roles.ts';
// Groups (ACC-021…ACC-032, ACC-060).
export { default as GroupPage } from './groups/GroupPage.svelte';
export { default as GroupsPage } from './groups/GroupsPage.svelte';
export { ensureGroups, groupKind, openGroup, type OpenedGroup } from './groups/groups.ts';
// Users: the directory and profiles (ACC-002…ACC-009, ACC-015, ACC-026, ACC-059, UI-010).
export { default as UserProfilePage } from './users/UserProfilePage.svelte';
export { default as UsersPage } from './users/UsersPage.svelte';
export { ensureDirectory, openProfile, type ProfileLookup } from './users/users.ts';
