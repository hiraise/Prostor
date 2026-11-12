// What the signed-in person may do (ACC-062): their effective permissions per scope and their
// capabilities, so screens hide actions that would be refused. The server decides every request on
// its own; this only shapes the interface. Loaded after the session and dropped with it.
import { api as defaultApi, type ApiClient } from '#lib/api/client.ts';
import type { AccessMap, CapabilityKey, PermissionKey, paths } from '#lib/api/schema.gen.ts';

let current = $state<AccessMap | null>(null);
let loaded: Promise<AccessMap> | null = null;
let client: ApiClient<paths> = defaultApi;

/** The access map, loaded once per session. A failed load is not remembered. */
export function ensureAccess(fetcher?: typeof fetch): Promise<AccessMap> {
  loaded ??= client
    .get('/users/me/access', { fetch: fetcher })
    .then((map) => {
      current = map;
      return map;
    })
    .catch((error: unknown) => {
      loaded = null;
      throw error;
    });
  return loaded;
}

/** Loads the map again: after a change that may have changed the person's own access. */
export async function refreshAccess(): Promise<void> {
  loaded = null;
  await ensureAccess();
}

/** Forgets the map: the session ended or another person signed in. */
export function clearAccess(): void {
  current = null;
  loaded = null;
}

/** The current map; `null` before it is loaded. Reactive. */
export function getAccess(): AccessMap | null {
  return current;
}

/** Holds the permission globally or, with `project`, in that project. Reactive. */
export function can(permission: PermissionKey, project?: number): boolean {
  if (!current) return false;
  if (current.global.includes(permission)) return true;
  if (project === undefined) return false;
  return current.projects.some(
    (entry) => entry.project_id === project && entry.permissions.includes(permission),
  );
}

/** Holds the permission globally or in at least one project. Reactive. */
export function canAnywhere(permission: PermissionKey): boolean {
  if (!current) return false;
  return (
    current.global.includes(permission) ||
    current.projects.some((entry) => entry.permissions.includes(permission))
  );
}

export function hasCapability(capability: CapabilityKey): boolean {
  return current?.capabilities.includes(capability) ?? false;
}

/** May open the user directory of the administration area (ACC-002). */
export function canSeeDirectory(): boolean {
  return can('user.directory');
}

/** May open the groups pages (ACC-022, ACC-051). */
export function canSeeGroups(): boolean {
  return can('system.read') || canAnywhere('project.update');
}

/** May open the roles pages (ACC-051, ACC-053). */
export function canSeeRoles(): boolean {
  return can('system.read') || canAnywhere('project.full');
}

/** May open the fields and value sets of the instance (FLD-025, FLD-027). */
export function canSeeFields(): boolean {
  return can('system.read') || canAnywhere('project.update');
}

/** May open the link types of the instance (LNK-018); changing them needs `system.write`. */
export function canSeeLinkTypes(): boolean {
  return can('system.read');
}

/** May open the mail settings of the instance (NTF-004); changing them needs `system.write`. */
export function canSeeMail(): boolean {
  return can('system.read');
}

/** May open the general settings of the instance (ADM-002); changing them needs `system.write`. */
export function canSeeSettings(): boolean {
  return can('system.read');
}

/** May open the capabilities of the instance (ACC-064); changing them needs `system.write`. */
export function canSeeCapabilities(): boolean {
  return can('system.read');
}

/** May open the jobs of the instance (ADM-031); acting on them needs `system.write`. */
export function canSeeJobs(): boolean {
  return can('system.read');
}

/**
 * The administration section is for people who manage something (ADM-001). Seeing the directory
 * alone does not count: every member does.
 */
export function canAdminister(): boolean {
  return (
    can('system.read') ||
    can('user.create') ||
    can('user.update') ||
    can('user.delete') ||
    canSeeGroups() ||
    canSeeRoles()
  );
}

/** For tests: a map as if loaded, and a client to use instead of the app's. */
export function setAccessForTests(map: AccessMap | null, api?: ApiClient<paths>): void {
  current = map;
  loaded = map ? Promise.resolve(map) : null;
  if (api) client = api;
}
