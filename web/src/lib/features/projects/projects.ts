// Projects in the data store (architecture.md §15) and the calls of the project pages: the list
// (PRJ-010, PRJ-012, PRJ-013), creating (PRJ-017, PRJ-018), settings (PRJ-038, PRJ-039), favorites,
// archive and restore (PRJ-061, PRJ-063), deletion (PRJ-064, PRJ-065), ownership (PRJ-053), the
// logo (PRJ-007) and the people of a project (PRJ-046…PRJ-052).
import { resolve } from '$app/paths';

import { ApiError, api } from '#lib/api/client.ts';
import type {
  IssueCount,
  NewProject,
  PageGroup,
  PageProject,
  PageUserRef,
  Project,
  ProjectCheck,
  ProjectPeople,
  TeamChange,
} from '#lib/api/schema.gen.ts';
import { uploadFile } from '#lib/api/upload.ts';
import { can, canAnywhere, refreshAccess } from '#lib/session/index.ts';
import { store, type EntityHandle, type ListHandle } from '#lib/store/index.ts';

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    project: Project;
  }
}

/** Projects per page; an instance has tens, sometimes hundreds. */
const PAGE = 50;
/** Team members per page. */
const MEMBERS_PAGE = 50;

store.defineKind('project', {
  async load(ids) {
    const answers = await Promise.all(
      ids.map((id) =>
        api
          .get('/projects/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    return answers.filter((project): project is Project => project !== null);
  },
});

/** Which part of the list (PRJ-013). */
export type ProjectSection = 'active' | 'archived' | 'deleting';

/** Projects of a section whose name contains `text` or whose key starts with it. */
export function ensureProjects(
  section: ProjectSection = 'active',
  text = '',
): Promise<ListHandle<Project>> {
  const q = text.trim();
  return store.ensureList('project', {
    key: ['projects', section, q],
    form: 'full',
    fetch: (cursor, signal) =>
      api.get('/projects', {
        query: { section, q: q || undefined, limit: PAGE, cursor: cursor ?? undefined },
        signal,
      }),
  });
}

/** One page of active projects for pickers; not kept in the store. */
export function findProjects(text: string, cursor?: string | null): Promise<PageProject> {
  return api.get('/projects', {
    query: { q: text.trim() || undefined, limit: PAGE, cursor: cursor ?? undefined },
  });
}

/** What the address of a project leads to. */
export type OpenedProject =
  | { kind: 'found'; project: EntityHandle<Project> }
  /** The address used a former key or the ID: go to the current key (PRJ-005). */
  | { kind: 'moved'; key: string }
  | { kind: 'missing' };

/**
 * The project of an address segment: its key, a former key, or its numeric ID (links from places
 * that know only the ID, such as a team in the groups list).
 */
export async function openProject(segment: string): Promise<OpenedProject> {
  try {
    if (/^\d{1,15}$/.test(segment)) {
      const handle = await store.ensureEntity('project', Number(segment), { form: 'full' });
      const found = handle.current;
      if (!found) return { kind: 'missing' };
      return { kind: 'moved', key: found.key };
    }
    const answer = await api.get('/projects/by-key/{key}', {
      path: { key: segment },
      accept: [404],
    });
    if (!('id' in answer)) return { kind: 'missing' };
    const project: Project = answer;
    store.entities.table('project').upsert(project, 'full');
    if (project.key !== segment) return { kind: 'moved', key: project.key };
    return {
      kind: 'found',
      project: await store.ensureEntity('project', project.id, { form: 'full' }),
    };
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_found') return { kind: 'missing' };
    throw error;
  }
}

/** The stored row of a project, if loaded. Reactive. */
export function projectRow(id: number): Project | undefined {
  return store.entities.table('project').get(id);
}

/** Addresses of a project's pages. */
export const projectHref = {
  overview: (project: Pick<Project, 'key'>) =>
    resolve('/(app)/projects/[key]', { key: project.key }),
  settings: (project: Pick<Project, 'key'>) =>
    resolve('/(app)/projects/[key]/settings/general', { key: project.key }),
  people: (project: Pick<Project, 'key'>) =>
    resolve('/(app)/projects/[key]/settings/people', { key: project.key }),
  fields: (project: Pick<Project, 'key'>) =>
    resolve('/(app)/projects/[key]/settings/fields', { key: project.key }),
};

/** May create projects (PRJ-017). Reactive. */
export function canCreateProjects(): boolean {
  return can('project.create');
}

/** May open the project's settings (PRJ-037): `project.update` there. Reactive. */
export function canConfigure(project: Pick<Project, 'id'>): boolean {
  return can('project.update', project.id);
}

/** May see the people of the project (PRJ-009): `project.full` there. Reactive. */
export function canSeePeople(project: Pick<Project, 'id'>): boolean {
  return can('project.full', project.id);
}

/** May delete the project (PRJ-064). Reactive. */
export function canDelete(project: Pick<Project, 'id'>): boolean {
  return can('project.delete', project.id);
}

/** May delete some project, so sees the projects being deleted (PRJ-065). Reactive. */
export function canDeleteSome(): boolean {
  return canAnywhere('project.delete');
}

/** Checks a name and a key before creating (PRJ-018); answers a free key for the name. */
export function checkProject(
  input: { name?: string; key?: string },
  signal?: AbortSignal,
): Promise<ProjectCheck> {
  return api.get('/projects/check', {
    query: {
      name: input.name === '' ? undefined : input.name,
      key: input.key === '' ? undefined : input.key,
    },
    signal,
  });
}

/**
 * Creates a project (PRJ-017). The creator now administers it, so their access is loaded again
 * before the project's pages open.
 */
export async function createProject(input: NewProject): Promise<Project> {
  const project = await api.post('/projects', { body: input });
  store.entities.table('project').upsert(project, 'full');
  await refreshAccess().catch(() => undefined);
  return project;
}

/** General settings a person changes (PRJ-038, PRJ-039). */
export interface ProjectSettings {
  name?: string;
  key?: string;
  description?: string;
  /** `null` clears the default audience. */
  default_audience_id?: number | null;
}

/**
 * Saves changed settings on the version the row holds. Throws `ConflictError` when someone else
 * changed the same settings meanwhile.
 */
export function updateProject(id: number, changes: ProjectSettings): Promise<Project> {
  const { default_audience_id: audience, ...shown } = changes;
  return store.update('project', id, shown, ({ changes: sent, version }) =>
    api.patch('/projects/{id}', {
      path: { id },
      body: {
        name: sent.name,
        key: sent.key,
        description: sent.description,
        ...(audience === undefined ? {} : { default_audience_id: audience }),
        version,
      },
    }),
  );
}

/** Shows a new logo address in the stored project; the version stays (a picture is no setting). */
function showLogo(project: Project, url: string | null): void {
  store.entities.table('project').upsert({ ...project, logo_url: url }, 'full');
}

/** Sends a picture and makes it the project's logo (PRJ-007). */
export async function uploadProjectLogo(project: Project, file: File): Promise<void> {
  const upload = await uploadFile(file);
  const picture = await api.put('/projects/{id}/logo', {
    path: { id: project.id },
    body: { upload: upload.id },
  });
  showLogo(project, picture.url);
}

/** Goes back to the generated logo: the key letters on the project's color (PRJ-007). */
export async function resetProjectLogo(project: Project): Promise<void> {
  await api.delete('/projects/{id}/logo', { path: { id: project.id } });
  showLogo(project, null);
}

/** Stars or unstars a project for the signed-in person (PRJ-012); the row shows it at once. */
export async function setFavorite(id: number, favorite: boolean): Promise<void> {
  const table = store.entities.table('project');
  const row = table.get(id);
  const before = row?.favorite;
  if (row) row.favorite = favorite;
  try {
    if (favorite) await api.put('/projects/{id}/favorite', { path: { id } });
    else await api.delete('/projects/{id}/favorite', { path: { id } });
  } catch (error) {
    const now = table.get(id);
    if (now && before !== undefined) now.favorite = before;
    throw error;
  }
}

/** Archives a project (PRJ-061). */
export async function archiveProject(id: number): Promise<Project> {
  const project = await api.post('/projects/{id}/archive', { path: { id } });
  store.entities.table('project').upsert(project, 'full');
  return project;
}

/** Restores an archived project (PRJ-063). */
export async function restoreProject(id: number): Promise<Project> {
  const project = await api.post('/projects/{id}/restore', { path: { id } });
  store.entities.table('project').upsert(project, 'full');
  return project;
}

/** How many of a project's issues the viewer sees, for the deletion warning (PRJ-064). */
export function projectIssueCount(id: number, signal?: AbortSignal): Promise<IssueCount> {
  return api.get('/issues/count', { query: { project: id }, signal });
}

/**
 * Starts deleting a project in the background (PRJ-064, PRJ-065). Nobody can open it any more, so
 * its row leaves the store; lists are loaded again by the caller.
 */
export async function deleteProject(id: number): Promise<void> {
  await api.delete('/projects/{id}', { path: { id } });
  store.entities.table('project').remove(id);
  await refreshAccess().catch(() => undefined);
}

/**
 * Starts a failed deletion again (PRJ-065). The row shows it queued at once; the list of
 * projects being deleted is loaded again by the caller.
 */
export async function restartDeletion(id: number): Promise<void> {
  await api.delete('/projects/{id}', { path: { id } });
  const row = store.entities.table('project').get(id);
  if (row) row.deletion = { state: 'queued', error: null };
}

/**
 * One page of groups and project teams whose name contains `text`, for the default audience of
 * new issues and articles (PRJ-039). Not kept in the store: the picker lives while it is open.
 */
export function findAudiences(text: string, cursor?: string | null): Promise<PageGroup> {
  return api.get('/groups', {
    query: {
      kind: 'all',
      q: text.trim() || undefined,
      limit: PAGE,
      cursor: cursor ?? undefined,
    },
  });
}

/** Gives the project a new owner, who becomes its administrator (PRJ-053). */
export async function transferProject(id: number, userId: number): Promise<Project> {
  const project = await api.post('/projects/{id}/owner', {
    path: { id },
    body: { user_id: userId },
  });
  store.entities.table('project').upsert(project, 'full');
  await refreshAccess().catch(() => undefined);
  return project;
}

/** The people page of a project (PRJ-046). Plain request: the page loads it again after a change. */
export function fetchPeople(id: number, signal?: AbortSignal): Promise<ProjectPeople> {
  return api.get('/projects/{id}/people', { path: { id }, signal });
}

/** Accounts added to the team directly, by name. */
export function findTeamUsers(
  id: number,
  text: string,
  cursor: string | null,
  signal?: AbortSignal,
): Promise<PageUserRef> {
  return api.get('/projects/{id}/team/users', {
    path: { id },
    query: { q: text.trim() || undefined, limit: MEMBERS_PAGE, cursor: cursor ?? undefined },
    signal,
  });
}

/**
 * Adds accounts and groups to the team (PRJ-048); returns those added now. The project row is
 * loaded again (team size) and so is the person's own access.
 */
export async function addToTeam(id: number, change: TeamChange): Promise<TeamChange> {
  const added = await api.post('/projects/{id}/team', { path: { id }, body: change });
  await afterTeamChange(id);
  return added;
}

/** Removes accounts and groups from the team (PRJ-050); returns those removed now. */
export async function removeFromTeam(id: number, change: TeamChange): Promise<TeamChange> {
  const removed = await api.post('/projects/{id}/team/remove', { path: { id }, body: change });
  await afterTeamChange(id);
  return removed;
}

async function afterTeamChange(id: number): Promise<void> {
  await Promise.all([
    store.refetch('project', [id]).catch(() => undefined),
    refreshAccess().catch(() => undefined),
  ]);
}

/**
 * Assigns a role in the project to an account, a group or the team (PRJ-051, PRJ-052). Who may:
 * `project.update` there and every permission of the role (ACC-050).
 */
export async function assignProjectRole(
  projectId: number,
  roleId: number,
  holder: { user_id: number } | { group_id: number },
): Promise<void> {
  await api.post('/role-assignments', {
    body: { role_id: roleId, ...holder, global: false, projects: [projectId] },
  });
  await refreshAccess().catch(() => undefined);
}

/** Revokes a role assignment made in the project. */
export async function revokeProjectRole(assignmentId: number): Promise<void> {
  await api.delete('/role-assignments/{id}', { path: { id: assignmentId } });
  await refreshAccess().catch(() => undefined);
}
