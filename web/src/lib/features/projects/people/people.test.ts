// Rules of the people page (PRJ-051, PRJ-052, PRJ-056, ACC-033).
import { describe, expect, it } from 'vitest';

import type { Assignment, GroupRef, ProjectPeople, Role } from '#lib/api/schema.gen.ts';

import { audienceOf, otherRoles, teamRoles } from './people.ts';

const TEAM: GroupRef = { id: 50, name: 'Web', builtin: null, team_of: 5 };
const ALL: GroupRef = { id: 2, name: 'All members', builtin: 'all_members', team_of: null };
const EVERYONE: GroupRef = { id: 1, name: 'Everyone', builtin: 'everyone', team_of: null };

function role(id: number, permissions: Role['permissions']): Role {
  return {
    id,
    name: `Role ${String(id)}`,
    builtin: null,
    description: '',
    permissions,
    assignments: 0,
    created_at: '2026-10-20T09:00:00Z',
    version: 1,
  };
}

const READER = role(1, ['project.basic', 'issue.read']);
const DIRECTORY = role(2, ['user.directory']);

function held(
  id: number,
  group: GroupRef,
  roleId: number,
  projectId: number | null = 5,
): Assignment {
  return {
    id,
    holder: { kind: 'group', group },
    role: { id: roleId, name: `Role ${String(roleId)}`, builtin: null },
    project_id: projectId,
    created_at: '2026-10-20T09:00:00Z',
  };
}

function people(extra: Partial<ProjectPeople> = {}): ProjectPeople {
  return {
    owner: null,
    team: TEAM,
    team_size: 1,
    team_groups: [],
    assignments: [held(10, TEAM, 1)],
    global: [],
    ...extra,
  };
}

describe('people rules', () => {
  it('PRJ-051, PRJ-052, ACC-033: team roles are those the team holds; the rest are direct', () => {
    const direct = held(11, ALL, 1);
    const all = people({ assignments: [held(10, TEAM, 1), direct] });
    expect(teamRoles(all).map((a) => a.id)).toEqual([10]);
    expect(otherRoles(all).map((a) => a.id)).toEqual([11]);
  });

  it('PRJ-056: a project is private unless a built-in group can see it', () => {
    const roles = [READER, DIRECTORY];
    expect(audienceOf(people(), roles)).toEqual({ kind: 'private' });
    // A role that gives nothing to read does not open it.
    expect(audienceOf(people({ global: [held(12, ALL, 2, null)] }), roles)).toEqual({
      kind: 'private',
    });
    expect(audienceOf(people({ global: [held(12, ALL, 1, null)] }), roles)).toEqual({
      kind: 'members',
      group: ALL,
    });
    expect(audienceOf(people({ team_groups: [ALL] }), roles)).toEqual({
      kind: 'members',
      group: ALL,
    });
    expect(
      audienceOf(
        people({ team_groups: [ALL], assignments: [held(10, TEAM, 1), held(13, EVERYONE, 1)] }),
        roles,
      ),
    ).toEqual({ kind: 'everyone', group: EVERYONE });
  });
});
