// The rules of a project's people page (PRJ-046…PRJ-056, ACC-033): which assignments are team
// roles, who sees the project, and how refusals read. Requests live in `../projects.ts`.
import { ApiError, api } from '#lib/api/client.ts';
import type {
  Assignment,
  GroupRef,
  PermissionKey,
  Project,
  ProjectPeople,
  Role,
} from '#lib/api/schema.gen.ts';
import { groupName, permissions, personName } from '#lib/features/access/index.ts';
import { refreshAccess } from '#lib/session/index.ts';
import { describeError } from '#lib/ui/toast.svelte.ts';

import * as m from './messages.ts';

/** How people being added get access: join the team, or hold one role in the project. */
export type AddMode = 'team' | 'role';

/** A project as a picker keeps it. */
export type ProjectChoice = Pick<Project, 'id' | 'name' | 'key' | 'logo_color'> &
  Partial<Pick<Project, 'logo_url'>>;

/** Roles the team holds in its project (PRJ-051, ACC-033). */
export function teamRoles(people: ProjectPeople): Assignment[] {
  return people.assignments.filter(
    (assignment) =>
      assignment.holder.kind === 'group' && assignment.holder.group.id === people.team.id,
  );
}

/** Roles held in the project by anyone but the team (PRJ-052). */
export function otherRoles(people: ProjectPeople): Assignment[] {
  return people.assignments.filter(
    (assignment) =>
      !(assignment.holder.kind === 'group' && assignment.holder.group.id === people.team.id),
  );
}

/** The name of whoever holds an assignment, as the viewer reads it. */
export function holderName(assignment: Pick<Assignment, 'holder'>): string {
  return assignment.holder.kind === 'user'
    ? personName(assignment.holder.user)
    : groupName(assignment.holder.group);
}

/**
 * Who sees the project beyond its team and role holders (PRJ-056): nobody else, every member
 * through "All members", or everyone with the guest through "Everyone". `group` names the built-in
 * group that opens it.
 */
export type Audience =
  | { kind: 'private' }
  | { kind: 'members'; group: GroupRef }
  | { kind: 'everyone'; group: GroupRef };

/**
 * The widest audience of the project: a built-in group that holds a role giving `project.basic`
 * here or globally, or that is in the team while a team role gives it. Roles are closed under
 * implication, so any reading permission includes `project.basic`.
 */
export function audienceOf(people: ProjectPeople, roles: readonly Role[]): Audience {
  const opens = (roleId: number): boolean =>
    roles.some((role) => role.id === roleId && role.permissions.includes('project.basic'));
  const groups: GroupRef[] = [];
  for (const assignment of [...people.assignments, ...people.global]) {
    if (assignment.holder.kind === 'group' && opens(assignment.role.id)) {
      groups.push(assignment.holder.group);
    }
  }
  if (teamRoles(people).some((assignment) => opens(assignment.role.id))) {
    groups.push(...people.team_groups);
  }
  const everyone = groups.find((group) => group.builtin === 'everyone');
  if (everyone) return { kind: 'everyone', group: everyone };
  const members = groups.find((group) => group.builtin === 'all_members');
  if (members) return { kind: 'members', group: members };
  return { kind: 'private' };
}

/** What went wrong, for a notice: the message and the names of missing permissions, if any. */
export interface Problem {
  message: string;
  missing: string[];
}

function isPermissionKey(key: string): key is PermissionKey {
  return Object.hasOwn(permissions, key);
}

/**
 * Explains a failed change. A refusal that names missing permissions lists them by name after
 * `lacking` (ACC-050); a conflict means the project is archived (its settings are read-only).
 */
export function problemOf(error: unknown, lacking?: () => string): Problem {
  if (error instanceof ApiError) {
    const missing = error.body?.missing_permissions ?? [];
    if (error.code === 'forbidden' && missing.length > 0 && lacking) {
      return {
        message: lacking(),
        missing: missing.map((key) => (isPermissionKey(key) ? permissions[key].name() : key)),
      };
    }
    if (error.code === 'conflict') return { message: m.archivedConflict(), missing: [] };
  }
  return { message: describeError(error).message, missing: [] };
}

/** One problem as a single line, for a toast. */
export function problemText(problem: Problem): string {
  return problem.missing.length > 0
    ? `${problem.message} ${problem.missing.join(', ')}.`
    : problem.message;
}

/** Who gets a role. */
export type Holder = { user_id: number } | { group_id: number };

/**
 * Assigns a role in the project to each holder in turn (PRJ-052) and stops at the first refusal;
 * `onassigned` hears of every holder done, so a retry skips them. The person's own access is
 * loaded again once.
 */
export async function assignRoles(
  projectId: number,
  roleId: number,
  holders: readonly Holder[],
  onassigned: (holder: Holder) => void = () => undefined,
): Promise<void> {
  let done = 0;
  try {
    for (const holder of holders) {
      await api.post('/role-assignments', {
        body: { role_id: roleId, ...holder, global: false, projects: [projectId] },
      });
      done += 1;
      onassigned(holder);
    }
  } finally {
    if (done > 0) await refreshAccess().catch(() => undefined);
  }
}
