// Roles in the data store (architecture.md §15; ACC-034, ACC-046, ACC-053…ACC-057): the `role`
// kind, the roles list with its filters, the fixed permission catalog, a role's assignments, and
// the calls that change roles.
import { ApiError, api } from '#lib/api/client.ts';
import type {
  Assignment,
  PermissionEntity,
  PermissionInfo,
  PermissionKey,
  PermissionOperation,
  Role,
} from '#lib/api/schema.gen.ts';
import { entities, operations, permissions } from '#lib/features/access/index.ts';
import { store, type EntityHandle, type ListHandle } from '#lib/store/index.ts';

/**
 * One assignment of a role as the store keeps it. Assignments never change once made (they are
 * only revoked), so they all have version 0.
 */
export type RoleAssignment = Assignment & { version: number };

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    role: Role;
    role_assignment: RoleAssignment;
  }
}

/** Roles per page; an instance has few, so one page usually holds them all. */
const PAGE = 100;
/** Assignments per page of a role's holders. */
const HOLDERS_PAGE = 50;

store.defineKind('role', {
  // There is no batch endpoint: a refetch loads each role; they are few at a time.
  async load(ids) {
    const found = await Promise.all(
      ids.map((id) =>
        api
          .get('/roles/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    return found.filter((role): role is Role => role !== null);
  },
});

store.defineKind('role_assignment', {
  // There is no endpoint for one assignment: they arrive with their role's list only.
  load: () => Promise.resolve([]),
});

/** What the roles list shows: part of a name and a permission ('' for any). */
export interface RoleFilters {
  q: string;
  permission: PermissionKey | '';
}

/** The filters of the roles page from its address (UI-006); unknown permissions are ignored. */
export function roleFilters(params: URLSearchParams): RoleFilters {
  const permission = params.get('permission') ?? '';
  return {
    q: params.get('q')?.trim() ?? '',
    permission: Object.hasOwn(permissions, permission) ? (permission as PermissionKey) : '',
  };
}

/** Roles matching the filters, predefined ones first (ACC-053). */
export function ensureRoles(filters: RoleFilters): Promise<ListHandle<Role>> {
  const q = filters.q.trim();
  return store.ensureList('role', {
    key: ['roles', { q, permission: filters.permission }],
    fetch: (cursor, signal) =>
      api.get('/roles', {
        query: {
          q: q || undefined,
          permission: filters.permission || undefined,
          limit: PAGE,
          cursor: cursor ?? undefined,
        },
        signal,
      }),
  });
}

/** One role; rejects with `not_found` when it does not exist or is hidden. */
export function ensureRole(id: number): Promise<EntityHandle<Role>> {
  return store.ensureEntity('role', id);
}

/** Who holds a role, at which scope (ACC-057). */
export function ensureRoleAssignments(roleId: number): Promise<ListHandle<RoleAssignment>> {
  return store.ensureList('role_assignment', {
    key: ['role', roleId],
    fetch: async (cursor, signal) => {
      const page = await api.get('/roles/{id}/assignments', {
        path: { id: roleId },
        query: { limit: HOLDERS_PAGE, cursor: cursor ?? undefined },
        signal,
      });
      return {
        items: page.items.map((assignment) => ({ ...assignment, version: 0 })),
        next_cursor: page.next_cursor,
      };
    },
  });
}

let catalog: Promise<PermissionInfo[]> | null = null;

/** The fixed permission catalog in catalog order (ACC-034), loaded once per page load. */
export function permissionCatalog(): Promise<PermissionInfo[]> {
  catalog ??= api
    .get('/permissions')
    .then((answer) => answer.permissions)
    .catch((error: unknown) => {
      catalog = null;
      throw error;
    });
  return catalog;
}

/** Puts a role the server returned into the store. */
export function keepRole(role: Role): Role {
  return store.entities.table('role').upsert(role);
}

/** Creates a custom role without permissions; they are switched on on its page (ACC-054). */
export async function createRole(name: string, description: string): Promise<Role> {
  const role = await api.post('/roles', { body: { name, description } });
  return keepRole(role);
}

/** Saves a custom role's name and description (ACC-054); `ConflictError` when both sides edited. */
export function saveRoleDetails(
  id: number,
  changes: { name: string; description: string },
): Promise<Role> {
  return store.update('role', id, changes, ({ changes: sent, version }) =>
    api.patch('/roles/{id}', {
      path: { id },
      body: { name: sent.name, description: sent.description, version },
    }),
  );
}

/**
 * Switches one permission of a custom role on (with what it implies) or off (with what depends on
 * it), ACC-046; the role the server returns replaces the stored one.
 */
export async function setRolePermission(
  id: number,
  permission: PermissionKey,
  on: boolean,
): Promise<Role> {
  const role = on
    ? await api.put('/roles/{id}/permissions/{permission}', { path: { id, permission } })
    : await api.delete('/roles/{id}/permissions/{permission}', { path: { id, permission } });
  return keepRole(role);
}

/**
 * Deletes a custom role and revokes its assignments (ACC-057). The stored row stays until
 * `forgetRole`, so the page can leave before the role disappears from it.
 */
export async function deleteRole(id: number): Promise<void> {
  await api.delete('/roles/{id}', { path: { id } });
}

/** Drops a deleted role from the store, and so from every list that shows it. */
export function forgetRole(id: number): void {
  store.entities.table('role').remove(id);
}

/** The permission keys the escalation guard named in a refusal (ACC-050); empty otherwise. */
export function missingPermissions(error: unknown): string[] {
  if (!(error instanceof ApiError) || error.code !== 'forbidden') return [];
  return error.body?.missing_permissions ?? [];
}

/** A permission's name in the viewer's language; an unknown key as it is. */
export function permissionName(key: string): string {
  return Object.hasOwn(permissions, key) ? permissions[key as PermissionKey].name() : key;
}

/** How the role editor groups the catalog (ACC-034). */
export type Grouping = 'entity' | 'operation';

export interface PermissionGroup {
  key: PermissionEntity | PermissionOperation;
  /** The group's heading in the viewer's language. */
  title: string;
  items: PermissionInfo[];
}

const OPERATIONS: readonly PermissionOperation[] = ['read', 'create', 'update', 'delete', 'link'];

/** The catalog in groups: by entity in catalog order, or by operation from reading to linking. */
export function groupPermissions(
  list: readonly PermissionInfo[],
  grouping: Grouping,
): PermissionGroup[] {
  const groups = new Map<PermissionEntity | PermissionOperation, PermissionGroup>();
  if (grouping === 'operation') {
    for (const operation of OPERATIONS) {
      groups.set(operation, { key: operation, title: operations[operation](), items: [] });
    }
  }
  for (const permission of list) {
    const key = grouping === 'entity' ? permission.entity : permission.operation;
    let group = groups.get(key);
    if (!group) {
      const title =
        grouping === 'entity' ? entities[permission.entity]() : operations[permission.operation]();
      group = { key, title, items: [] };
      groups.set(key, group);
    }
    group.items.push(permission);
  }
  return [...groups.values()].filter((group) => group.items.length > 0);
}

/**
 * The role's permissions after switching one on or off as the server does it (ACC-046): on adds
 * what it implies, off removes what depends on it. Lets a toggle show at once.
 */
export function toggled(
  current: ReadonlySet<PermissionKey>,
  permission: PermissionInfo,
  on: boolean,
): Set<PermissionKey> {
  const next = new Set(current);
  if (on) for (const key of [permission.key, ...permission.implies]) next.add(key);
  else for (const key of [permission.key, ...permission.dependents]) next.delete(key);
  return next;
}
