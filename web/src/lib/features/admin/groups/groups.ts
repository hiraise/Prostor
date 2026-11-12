// Groups in the data store (architecture.md §15) and the calls of the groups pages: the list
// (ACC-022), creating (ACC-023), settings (ACC-024), members (ACC-026), deletion with a
// replacement (ACC-032) and the group's roles (ACC-060).
import { ApiError, api } from '#lib/api/client.ts';
import type {
  Group,
  GroupDeletionImpact,
  NewGroup,
  PageUserRef,
  PermissionKey,
} from '#lib/api/schema.gen.ts';
import { permissions, type GroupKind } from '#lib/features/access/index.ts';
import { can, canAnywhere, currentUser, refreshAccess } from '#lib/session/index.ts';
import { store, type EntityHandle, type ListHandle } from '#lib/store/index.ts';
import { describeError } from '#lib/ui/toast.svelte.ts';

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    group: Group;
  }
}

/** Groups per page of the list; an instance has tens, rarely hundreds. */
const PAGE = 50;
/** Members per page of a group. */
const MEMBERS_PAGE = 50;

/** IDs of deleted groups that the server answered with their replacement (ACC-013). */
const replacements = new Map<number, number>();

store.defineKind('group', {
  async load(ids) {
    const answers = await Promise.all(
      ids.map((id) =>
        api
          .get('/groups/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    const found: Group[] = [];
    answers.forEach((group, index) => {
      const id = ids[index];
      if (!group || id === undefined) return;
      if (group.id === id) {
        found.push(group);
      } else {
        // The ID of a deleted group: keep the replacement under its own ID.
        replacements.set(id, group.id);
        store.entities.table('group').upsert(group, 'full');
      }
    });
    return found;
  },
});

/**
 * Groups whose name contains `text`, built-in ones first, then by name: groups of people, project
 * teams or both (ACC-022).
 */
export function ensureGroups(text = '', kind: GroupKind = 'groups'): Promise<ListHandle<Group>> {
  const q = text.trim();
  return store.ensureList('group', {
    key: ['groups', q, kind],
    form: 'full',
    fetch: (cursor, signal) =>
      api.get('/groups', {
        query: { q: q || undefined, kind, limit: PAGE, cursor: cursor ?? undefined },
        signal,
      }),
  });
}

/** The kind of groups the list shows, from its address (UI-006); groups of people by default. */
export function groupKind(value: string | null): GroupKind {
  return value === 'teams' || value === 'all' ? value : 'groups';
}

/** What the address of a group leads to. */
export type OpenedGroup =
  | { kind: 'found'; group: EntityHandle<Group> }
  | { kind: 'replaced'; id: number }
  | { kind: 'missing' };

/** One group for its page; a deleted group's ID leads to its replacement (ACC-013). */
export async function openGroup(id: number): Promise<OpenedGroup> {
  try {
    return { kind: 'found', group: await store.ensureEntity('group', id, { form: 'full' }) };
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_found') {
      const replacement = replacements.get(id);
      return replacement === undefined
        ? { kind: 'missing' }
        : { kind: 'replaced', id: replacement };
    }
    throw error;
  }
}

/** The stored row of a group, if loaded. Reactive. */
export function groupRow(id: number): Group | undefined {
  return store.entities.table('group').get(id);
}

/** May create, edit and delete groups and change their members (ACC-051). Reactive. */
export function canManageGroups(): boolean {
  return can('system.write') || canAnywhere('project.update');
}

/** May assign and revoke roles globally (ACC-058). Reactive. */
export function canAssignGlobally(): boolean {
  return can('system.write');
}

/**
 * May assign roles to the group at some scope (ACC-060): globally, or in a project they manage. A
 * project team holds roles only in its own project (ACC-033). Reactive.
 */
export function canAssignTo(group: Pick<Group, 'team_of'>): boolean {
  if (group.team_of !== null) return can('project.update', group.team_of);
  return canAssignGlobally() || canAnywhere('project.update');
}

export async function createGroup(input: NewGroup): Promise<Group> {
  const group = await api.post('/groups', { body: input });
  store.entities.table('group').upsert(group, 'full');
  return group;
}

/** Settings a person changes (ACC-024). */
export interface GroupSettings {
  name?: string;
  description?: string;
  auto_join?: boolean;
}

/**
 * Saves changed settings on the version the row holds; the row shows them at once. Throws
 * `ConflictError` when someone else changed the same settings meanwhile.
 */
export function updateGroup(id: number, changes: GroupSettings): Promise<Group> {
  return store.update('group', id, changes, ({ changes: sent, version }) =>
    api.patch('/groups/{id}', {
      path: { id },
      body: {
        name: sent.name,
        description: sent.description,
        auto_join: sent.auto_join,
        version,
      },
    }),
  );
}

/** Loads a group again after a change of its members or roles; a failure leaves the row. */
async function reload(id: number): Promise<void> {
  await store.refetch('group', [id]).catch(() => undefined);
}

/** The signed-in person's own access may have changed. */
function accessMayHaveChanged(): void {
  void refreshAccess().catch(() => undefined);
}

function includesMe(users: readonly number[]): boolean {
  const me = currentUser()?.id;
  return me !== undefined && users.includes(me);
}

/** Members by name; built-in groups list every account they include. */
export function findMembers(
  id: number,
  text: string,
  cursor: string | null,
  signal?: AbortSignal,
): Promise<PageUserRef> {
  return api.get('/groups/{id}/members', {
    path: { id },
    query: { q: text.trim() || undefined, limit: MEMBERS_PAGE, cursor: cursor ?? undefined },
    signal,
  });
}

/** Adds accounts; returns those that were not members yet. */
export async function addMembers(id: number, users: number[]): Promise<number[]> {
  const answer = await api.post('/groups/{id}/members', { path: { id }, body: { users } });
  await reload(id);
  if (includesMe(answer.users)) accessMayHaveChanged();
  return answer.users;
}

export async function removeMembers(id: number, users: number[]): Promise<number[]> {
  const answer = await api.post('/groups/{id}/members/remove', { path: { id }, body: { users } });
  await reload(id);
  if (includesMe(answer.users)) accessMayHaveChanged();
  return answer.users;
}

/** Assigns a role to the group globally or in projects (ACC-060). */
export async function assignGroupRole(
  groupId: number,
  roleId: number,
  scope: { global: true } | { projects: number[] },
): Promise<void> {
  await api.post('/role-assignments', {
    body: {
      role_id: roleId,
      group_id: groupId,
      global: 'global' in scope,
      projects: 'projects' in scope ? scope.projects : [],
    },
  });
  await reload(groupId);
  accessMayHaveChanged();
}

export async function revokeGroupRole(groupId: number, assignmentId: number): Promise<void> {
  await api.delete('/role-assignments/{id}', { path: { id: assignmentId } });
  await reload(groupId);
  accessMayHaveChanged();
}

/** What deleting these groups takes away. */
export function deletionImpact(groups: number[]): Promise<GroupDeletionImpact> {
  return api.post('/groups/deletion-impact', { body: { groups } });
}

/**
 * Deletes groups; `replacement` takes over what was limited to them. The rows stay until
 * `forgetGroups`, so a page that shows one can leave first.
 */
export async function deleteGroups(groups: number[], replacement: number): Promise<void> {
  await api.post('/groups/delete', { body: { groups, replacement } });
  for (const id of groups) replacements.set(id, replacement);
  accessMayHaveChanged();
}

/** Drops deleted groups from the store. */
export function forgetGroups(ids: readonly number[]): void {
  const table = store.entities.table('group');
  for (const id of ids) table.remove(id);
}

function isPermissionKey(key: string): key is PermissionKey {
  return Object.hasOwn(permissions, key);
}

/** What went wrong, for a notice: the message and the names of missing permissions, if any. */
export interface Problem {
  message: string;
  missing: string[];
}

/**
 * Explains a failed call. A refusal that names missing permissions lists them by name after
 * `lacking`; a conflict says `conflict` when the call site knows what it means, and the
 * last-administrator guard says `lastAdministrator`.
 */
export function problemOf(
  error: unknown,
  texts: { lacking?: () => string; conflict?: () => string; lastAdministrator?: () => string } = {},
): Problem {
  if (error instanceof ApiError) {
    const missing = error.body?.missing_permissions ?? [];
    if (error.code === 'forbidden' && missing.length > 0 && texts.lacking) {
      return {
        message: texts.lacking(),
        missing: missing.map((key) => (isPermissionKey(key) ? permissions[key].name() : key)),
      };
    }
    if (error.code === 'conflict' && texts.conflict) {
      return { message: texts.conflict(), missing: [] };
    }
    if (error.code === 'last_administrator' && texts.lastAdministrator) {
      return { message: texts.lastAdministrator(), missing: [] };
    }
  }
  return { message: describeError(error).message, missing: [] };
}
