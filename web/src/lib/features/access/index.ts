// People, groups and roles as every screen shows them (ACC-006, ACC-016, ACC-034…ACC-070):
// names in the viewer's language, the `account` store kind, people search and pickers.
export { default as PeopleSearch } from './PeopleSearch.svelte';
export { default as PersonName } from './PersonName.svelte';
export * as accessMessages from './messages.ts';
export {
  builtinGroups,
  disabledLabels,
  entities,
  groupName,
  operations,
  permissions,
  predefinedRoles,
  roleName,
} from './names.ts';
export {
  allRoles,
  findGroups,
  globalOnlyPermissions,
  projectPermissions,
  worksInProjects,
  type GroupKind,
} from './lists.ts';
export {
  ensureAccount,
  findPeople,
  keepAccount,
  personName,
  personUsername,
  type AccountState,
  type PersonLike,
} from './people.ts';
