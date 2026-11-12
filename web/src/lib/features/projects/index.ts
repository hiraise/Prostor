// Projects (PRJ): the list, the creation wizard, the project frame and its pages, settings and
// people. Other features use projects only through this file.
export { default as CreateProjectPage } from './create/CreateProjectPage.svelte';
export { default as GeneralSettings } from './general/GeneralSettings.svelte';
export * as projectMessages from './messages.ts';
export { default as ProjectOverview } from './overview/ProjectOverview.svelte';
export { default as ProjectFrame } from './ProjectFrame.svelte';
export { default as ProjectsPage } from './list/ProjectsPage.svelte';
export { default as ProjectLogo } from './ProjectLogo.svelte';
export { default as RulesTab } from './rules/RulesTab.svelte';
export { default as SettingsNav } from './SettingsNav.svelte';
// The people page (PRJ-046…PRJ-056) and projects as the access screens show and pick them.
export { default as PeoplePage } from './people/PeoplePage.svelte';
export { default as ProjectLabel } from './people/ProjectLabel.svelte';
export { default as ProjectPicker } from './people/ProjectPicker.svelte';
export type { ProjectChoice } from './people/people.ts';
export {
  addToTeam,
  archiveProject,
  assignProjectRole,
  canConfigure,
  canCreateProjects,
  canDelete,
  canDeleteSome,
  canSeePeople,
  checkProject,
  createProject,
  deleteProject,
  ensureProjects,
  fetchPeople,
  findAudiences,
  findProjects,
  findTeamUsers,
  openProject,
  projectHref,
  projectRow,
  removeFromTeam,
  restartDeletion,
  restoreProject,
  revokeProjectRole,
  setFavorite,
  transferProject,
  updateProject,
  type OpenedProject,
  type ProjectSection,
  type ProjectSettings,
} from './projects.ts';
