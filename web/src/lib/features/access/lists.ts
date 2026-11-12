// Groups and roles for pickers (ACC-026, ACC-059): plain requests, not kept in the store, since a
// picker lives only while its dialog is open.
import { api } from '#lib/api/client.ts';
import type { PageGroup, PageRole, PermissionKey, Role } from '#lib/api/schema.gen.ts';

/** Which groups a search includes: groups of people (the default), project teams, or both. */
export type GroupKind = 'groups' | 'teams' | 'all';

/** One page of groups whose name contains `text`. */
export function findGroups(
  text: string,
  cursor?: string | null,
  kind: GroupKind = 'groups',
): Promise<PageGroup> {
  return api.get('/groups', {
    query: { q: text.trim() || undefined, cursor: cursor ?? undefined, limit: 50, kind },
  });
}

/** Every role (an instance has few). */
export async function allRoles(): Promise<Role[]> {
  const roles: Role[] = [];
  let cursor: string | null = null;
  do {
    const page: PageRole = await api.get('/roles', {
      query: { cursor: cursor ?? undefined, limit: 100 },
    });
    roles.push(...page.items);
    cursor = page.next_cursor;
  } while (cursor);
  return roles;
}

let globalOnly: Promise<ReadonlySet<PermissionKey>> | null = null;

/**
 * Permissions that work only in a role assigned globally (ACC-044), from the catalog; loaded once
 * per page load.
 */
export function globalOnlyPermissions(): Promise<ReadonlySet<PermissionKey>> {
  globalOnly ??= api
    .get('/permissions')
    .then(
      (answer) =>
        new Set(answer.permissions.filter((info) => info.global_only).map((info) => info.key)),
    )
    .catch((error: unknown) => {
      globalOnly = null;
      throw error;
    });
  return globalOnly;
}

/** The permissions a role gives when it is assigned in a project (ACC-045). */
export function projectPermissions(
  role: Pick<Role, 'permissions'>,
  globalOnlyKeys: ReadonlySet<PermissionKey>,
): PermissionKey[] {
  return role.permissions.filter((key) => !globalOnlyKeys.has(key));
}

/** A role gives something in a project, so it may be assigned there (PRJ-052). */
export function worksInProjects(
  role: Pick<Role, 'permissions'>,
  globalOnlyKeys: ReadonlySet<PermissionKey>,
): boolean {
  return projectPermissions(role, globalOnlyKeys).length > 0;
}
