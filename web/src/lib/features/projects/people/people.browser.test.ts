// The people page of a project on a desktop and on a phone (step 1.2; PRJ-046…PRJ-054, PRJ-056,
// ACC-033).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  AccessMap,
  AccountSummary,
  Assignment,
  Group,
  GroupRef,
  PermissionInfo,
  PermissionKey,
  Project,
  ProjectPeople,
  Role,
  Session,
  UserRef,
} from '#lib/api/schema.gen.ts';
import { permissions } from '#lib/features/access/index.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeAnswer, type FakeRoute } from '../../../../test/fake-api.ts';
import PeoplePage from './PeoplePage.svelte';

const SESSION: Session = {
  user: {
    id: 1,
    username: 'anna',
    full_name: 'Anna Admin',
    email: 'anna@example.com',
    email_verified: true,
    avatar_color: 0,
    avatar_url: null,
    version: 1,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

const PROJECT_RIGHTS: PermissionKey[] = [
  'project.basic',
  'project.full',
  'project.update',
  'issue.read',
  'issue.create',
];

/** Manages project 5 and sees people and groups. */
const MANAGER: AccessMap = {
  global: ['user.directory', 'user.self'],
  projects: [{ project_id: 5, permissions: PROJECT_RIGHTS }],
  capabilities: [],
};

/** Sees the project's people, changes nothing. */
const VIEWER: AccessMap = {
  global: ['user.directory'],
  projects: [{ project_id: 5, permissions: ['project.basic', 'project.full'] }],
  capabilities: [],
};

const GLOBAL_ONLY: PermissionKey[] = [
  'system.read',
  'system.write',
  'user.directory',
  'user.details',
  'user.self',
  'user.update',
  'user.create',
  'user.delete',
  'project.create',
];

const CATALOG: PermissionInfo[] = (Object.keys(permissions) as PermissionKey[]).map((key) => ({
  key,
  entity: 'project',
  operation: 'read',
  global_only: GLOBAL_ONLY.includes(key),
  implies: [],
  dependents: [],
}));

function ref(id: number, fullName: string, username: string): UserRef {
  return {
    id,
    full_name: fullName,
    username,
    avatar_color: id % 8,
    avatar_url: null,
    disabled: false,
    kind: 'person',
    pseudonym: null,
  };
}

function account(id: number, fullName: string, username: string): AccountSummary {
  return {
    id,
    full_name: fullName,
    username,
    avatar_color: id % 8,
    avatar_url: null,
    created_at: '2026-10-20T09:00:00Z',
    disabled: null,
    email: `${username}@example.com`,
    email_verified: true,
    full_profile: true,
    groups: [],
    projects: [],
    guest: false,
    invited: false,
    version: 1,
  };
}

function group(id: number, name: string, extra: Partial<Group> = {}): Group {
  return {
    id,
    name,
    builtin: null,
    team_of: null,
    description: '',
    auto_join: false,
    members: 2,
    roles: [],
    created_at: '2026-10-20T09:00:00Z',
    version: 1,
    ...extra,
  };
}

function role(id: number, name: string, extra: Partial<Role> = {}): Role {
  return {
    id,
    name,
    builtin: null,
    description: '',
    permissions: [],
    assignments: 0,
    created_at: '2026-10-20T09:00:00Z',
    version: 1,
    ...extra,
  };
}

const ANNA = ref(1, 'Anna Admin', 'anna');
const BORIS = ref(2, 'Boris Petrov', 'boris');
const CLARA = ref(3, 'Clara Smith', 'clara');
const DANA = ref(4, 'Dana Observer', 'dana');

const TEAM: GroupRef = { id: 50, name: 'Web', builtin: null, team_of: 5 };
const DEVELOPERS: GroupRef = { id: 3, name: 'Developers', builtin: null, team_of: null };
const ALL_MEMBERS: GroupRef = { id: 2, name: 'All members', builtin: 'all_members', team_of: null };

const ROLES: Role[] = [
  role(1, 'System administrator', {
    builtin: 'system_admin',
    permissions: ['system.read', 'system.write', 'project.basic', 'project.update'],
  }),
  role(2, 'Project administrator', {
    builtin: 'project_admin',
    permissions: ['project.basic', 'project.full', 'project.update', 'issue.read'],
  }),
  role(3, 'Project member', {
    builtin: 'project_member',
    permissions: ['project.basic', 'issue.read', 'issue.create'],
  }),
  role(4, 'Directory viewer', {
    builtin: 'directory_viewer',
    permissions: ['user.directory', 'user.self'],
  }),
  role(12, 'Observer', {
    description: 'Reads along',
    permissions: ['project.basic', 'issue.read'],
  }),
  role(13, 'Release manager', {
    description: 'Ships releases',
    permissions: ['project.basic', 'issue.read', 'issue.update', 'issue.delete'],
  }),
];

function assignment(
  id: number,
  holder: Assignment['holder'],
  roleId: number,
  projectId: number | null = 5,
): Assignment {
  const found = ROLES.find((item) => item.id === roleId);
  if (!found) throw new Error(`no role ${String(roleId)}`);
  return {
    id,
    holder,
    role: { id: found.id, name: found.name, builtin: found.builtin },
    project_id: projectId,
    created_at: '2026-10-20T09:00:00Z',
  };
}

function project(extra: Partial<Project> = {}): Project {
  return {
    id: 5,
    name: 'Web',
    key: 'WEB',
    kind: 'standard',
    template: 'blank',
    description: '',
    description_html: '',
    archived_at: null,
    created_at: '2026-10-20T09:00:00Z',
    default_audience: null,
    deletion: null,
    favorite: false,
    former_keys: [],
    logo_color: 2,
    logo_url: null,
    next_number: 1,
    owner: ANNA,
    team: TEAM,
    team_size: 4,
    version: 1,
    ...extra,
  };
}

function people(extra: Partial<ProjectPeople> = {}): ProjectPeople {
  return {
    owner: ANNA,
    team: TEAM,
    team_size: 4,
    team_groups: [DEVELOPERS],
    assignments: [
      assignment(100, { kind: 'group', group: TEAM }, 3),
      assignment(101, { kind: 'user', user: DANA }, 12),
    ],
    global: [assignment(102, { kind: 'user', user: ANNA }, 1, null)],
    ...extra,
  };
}

function forbiddenFor(missing: string[]): FakeAnswer {
  return {
    status: 403,
    body: { code: 'forbidden', message: 'missing permissions', missing_permissions: missing },
  };
}

/** The routes the page needs, with `routes` added or replacing them. */
function server(access: AccessMap, routes: Record<string, FakeRoute>): ReturnType<typeof fakeApi> {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi({
    'GET /users/me/access': { body: access },
    'GET /roles': { body: { items: ROLES, next_cursor: null } },
    'GET /permissions': { body: { permissions: CATALOG } },
    'GET /projects/5/people': { body: people() },
    'GET /projects/5/team/users': { body: { items: [BORIS, CLARA], next_cursor: null } },
    'GET /projects/5': { body: project() },
    ...routes,
  });
}

/** Buttons and links smaller than a touch target. */
function tooSmall(root: Element): string[] {
  return smallTargets(root, isPhone() ? 44 : 28);
}

function toastTexts(): string[] {
  return toasts.items.map((toast) => toast.message);
}

/** A field by its label; required fields add "*" to the label text. */
function label(text: string): RegExp {
  return new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\*?$`);
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('PeoplePage', () => {
  it('PRJ-046, PRJ-047, PRJ-051, PRJ-054, PRJ-056: shows the owner, the team, other roles and global ones', async () => {
    server(MANAGER, {});
    const screen = await render(PeoplePage, { project: project() });

    await expect.element(screen.getByRole('heading', { level: 1, name: 'People' })).toBeVisible();
    await expect
      .element(
        screen.getByText('Private project: only its team and people with roles in it see it.'),
      )
      .toBeVisible();

    const owner = screen.getByRole('region', { name: 'Owner' });
    await expect
      .element(owner.getByRole('link', { name: 'Anna Admin' }))
      .toHaveAttribute('href', '/users/1');
    await expect.element(owner.getByRole('button', { name: 'Transfer ownership' })).toBeVisible();

    const team = screen.getByRole('region', { name: 'Team · 4 people' });
    await expect.element(team.getByRole('heading', { name: 'Team · 4 people' })).toBeVisible();
    await expect
      .element(team.getByText('the list of assignees includes the team', { exact: false }))
      .toBeVisible();
    const roles = team.getByRole('list', { name: 'Roles of the team' });
    await expect.element(roles.getByText('Project member')).toBeVisible();
    await expect
      .element(team.getByRole('link', { name: 'Developers' }))
      .toHaveAttribute('href', '/admin/groups/3');
    const direct = team.getByRole('region', { name: 'Added directly' });
    await expect.element(direct.getByRole('link', { name: 'Boris Petrov' })).toBeVisible();
    await expect.element(direct.getByRole('link', { name: 'Clara Smith' })).toBeVisible();

    const other = screen.getByRole('list', { name: 'Roles outside the team' });
    await expect.element(other.getByText('Dana Observer')).toBeVisible();
    await expect.element(other.getByText('Observer', { exact: true })).toBeVisible();
    await expect
      .element(other.getByRole('button', { name: 'Revoke “Observer” from Dana Observer' }))
      .toBeVisible();

    const global = screen.getByRole('list', { name: 'Global roles here' });
    await expect.element(global.getByText('System administrator')).toBeVisible();
    // Global roles change on their holder's page only.
    expect(global.getByRole('button').query()).toBeNull();

    expect(tooSmall(screen.container)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('PRJ-046: people who may only see it get no actions, in Russian too', async () => {
    setLocale('ru');
    server(VIEWER, {});
    const screen = await render(PeoplePage, { project: project() });

    await expect.element(screen.getByRole('heading', { level: 1, name: 'Люди' })).toBeVisible();
    await expect.element(screen.getByText('Команда · 4\u00a0человека')).toBeVisible();
    await expect.element(screen.getByText('Участник проекта')).toBeVisible();
    await expect.element(screen.getByRole('link', { name: 'Boris Petrov' })).toBeVisible();
    await expect
      .element(screen.getByText('Вы видите, у кого есть доступ к проекту', { exact: false }))
      .toBeVisible();
    expect(screen.getByRole('button').query()).toBeNull();
    await expectAccessible(screen.container);
  });

  it('PRJ-046: an archived project shows its people and hides every change', async () => {
    server(MANAGER, {});
    const screen = await render(PeoplePage, {
      project: project({ archived_at: '2026-10-21T09:00:00Z' }),
    });

    await expect.element(screen.getByText('Dana Observer')).toBeVisible();
    await expect.element(screen.getByText('Boris Petrov')).toBeVisible();
    expect(screen.getByRole('button').query()).toBeNull();
  });

  it('PRJ-056: a built-in group in the team opens the project to every member', async () => {
    server(MANAGER, {
      'GET /projects/5/people': { body: people({ team_groups: [ALL_MEMBERS] }) },
    });
    const screen = await render(PeoplePage, { project: project() });

    await expect
      .element(screen.getByText('Every member sees this project: “All members” has access to it.'))
      .toBeVisible();
  });

  it('PRJ-051, ACC-033: adds a team role among project roles, a refusal names permissions, removes one', async () => {
    let current = people();
    let attempts = 0;
    const calls = server(MANAGER, {
      'GET /projects/5/people': () => ({ body: current }),
      'POST /role-assignments': () => {
        if (++attempts === 1) return forbiddenFor(['issue.delete']);
        current = {
          ...current,
          assignments: [
            ...current.assignments,
            assignment(103, { kind: 'group', group: TEAM }, 13),
          ],
        };
        return { status: 201, body: { items: [] } };
      },
      'DELETE /role-assignments/100': () => {
        current = { ...current, assignments: current.assignments.filter((a) => a.id !== 100) };
        return { status: 204 };
      },
    });
    const screen = await render(PeoplePage, { project: project() });
    const team = screen.getByRole('region', { name: 'Team roles' });

    await team.getByRole('button', { name: 'Add role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add a team role' });
    const select = dialog.getByLabelText(label('Role'));
    await expect.element(select).toBeVisible();
    // Held team roles and roles that give nothing in a project are not offered.
    expect(dialog.getByRole('option', { name: 'Project member' }).query()).toBeNull();
    expect(dialog.getByRole('option', { name: 'Directory viewer' }).query()).toBeNull();
    await select.selectOptions('13');
    await expect.element(dialog.getByText('Ships releases')).toBeVisible();
    await expect.element(dialog.getByText('Delete issues')).toBeVisible();
    await dialog.getByRole('button', { name: 'Add role' }).click();
    await expect
      .element(
        dialog.getByText('You can pass on only permissions you hold yourself', { exact: false }),
      )
      .toBeVisible();
    await expect
      .element(dialog.getByRole('listitem').filter({ hasText: 'Delete issues' }).last())
      .toBeVisible();
    await expectAccessible(dialog.element());

    await dialog.getByRole('button', { name: 'Add role' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.filter((call) => call.path === '/role-assignments').at(-1)?.body).toEqual({
      role_id: 13,
      group_id: 50,
      global: false,
      projects: [5],
    });
    const chips = screen.getByRole('list', { name: 'Roles of the team' });
    await expect.element(chips.getByText('Release manager')).toBeVisible();
    expect(toastTexts()).toContain('Team role added');

    await chips.getByRole('button', { name: 'Remove the team role “Project member”' }).click();
    const confirm = page.getByRole('dialog', { name: 'Remove the team role “Project member”?' });
    await expect
      .element(
        confirm.getByText('Team members lose its permissions in this project', { exact: false }),
      )
      .toBeVisible();
    await confirm.getByRole('button', { name: 'Remove role' }).click();
    await expect.element(confirm).not.toBeInTheDocument();
    await expect.element(chips.getByText('Project member')).not.toBeInTheDocument();
    expect(
      calls.some((call) => call.method === 'DELETE' && call.path === '/role-assignments/100'),
    ).toBe(true);
    expect(tooSmall(screen.container)).toEqual([]);
  });

  it('PRJ-048, PRJ-056: adds people and groups to the team; a refusal names the missing permissions', async () => {
    let current = people();
    let attempts = 0;
    const calls = server(MANAGER, {
      'GET /projects/5/people': () => ({ body: current }),
      'GET /users': {
        body: {
          items: [account(2, 'Boris Petrov', 'boris'), account(6, 'Fedor Novak', 'fedor')],
          next_cursor: null,
        },
      },
      'GET /groups': {
        body: {
          items: [
            group(2, 'All members', { builtin: 'all_members', auto_join: true }),
            group(3, 'Developers'),
            group(7, 'QA'),
            group(60, 'Mobile', { team_of: 6 }),
          ],
          next_cursor: null,
        },
      },
      'POST /projects/5/team': () => {
        if (++attempts === 1) return forbiddenFor(['issue.create', 'project.update']);
        current = {
          ...current,
          team_size: 6,
          team_groups: [DEVELOPERS, { ...DEVELOPERS, id: 7, name: 'QA' }],
        };
        return { body: { users: [6], groups: [7] } };
      },
    });
    const screen = await render(PeoplePage, { project: project() });

    await screen.getByRole('button', { name: 'Add people' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add people to “Web”' });
    await expect
      .element(dialog.getByRole('tab', { name: 'People' }))
      .toHaveAttribute('aria-selected', 'true');
    // People added directly already are not offered for joining.
    await expect.element(dialog.getByRole('checkbox', { name: /^Fedor Novak/ })).toBeVisible();
    expect(dialog.getByRole('checkbox', { name: /^Boris Petrov/ }).query()).toBeNull();
    await dialog.getByRole('checkbox', { name: /^Fedor Novak/ }).click();

    await dialog.getByRole('tab', { name: 'Groups' }).click();
    // Groups in the team already and project teams are not offered.
    await expect.element(dialog.getByRole('checkbox', { name: /^QA/ })).toBeVisible();
    expect(dialog.getByRole('checkbox', { name: /^Developers/ }).query()).toBeNull();
    expect(dialog.getByRole('checkbox', { name: /Mobile/ }).query()).toBeNull();
    await dialog.getByRole('checkbox', { name: /^All members/ }).click();
    await expect
      .element(dialog.getByText('“All members” includes every account', { exact: false }))
      .toBeVisible();
    await dialog.getByRole('checkbox', { name: /^All members/ }).click();
    await dialog.getByRole('checkbox', { name: /^QA/ }).click();
    await expect.element(dialog.getByText('Chosen: 1 person, 1 group')).toBeVisible();
    await expect.element(dialog.getByRole('radio', { name: 'Join the team' })).toBeChecked();
    await expect
      .element(dialog.getByText('They get the team roles: Project member.'))
      .toBeVisible();
    await expectAccessible(dialog.element());

    await dialog.getByRole('button', { name: 'Add to team' }).click();
    await expect
      .element(
        dialog.getByText('To add people to the team, you need every permission its roles give', {
          exact: false,
        }),
      )
      .toBeVisible();
    await expect.element(dialog.getByText('Create issues')).toBeVisible();
    await expect.element(dialog.getByText('Manage projects')).toBeVisible();

    await dialog.getByRole('button', { name: 'Add to team' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.filter((call) => call.path === '/projects/5/team').at(-1)?.body).toEqual({
      users: [6],
      groups: [7],
    });
    await expect.element(screen.getByText('Team · 6 people')).toBeVisible();
    await expect.element(screen.getByRole('link', { name: 'QA' })).toBeVisible();
    expect(toastTexts()).toContain('2 new members of the team');
  });

  it('PRJ-048: nobody chosen is said in the dialog without calling the server', async () => {
    const calls = server(MANAGER, {
      'GET /users': { body: { items: [], next_cursor: null } },
      'GET /groups': { body: { items: [], next_cursor: null } },
    });
    const screen = await render(PeoplePage, { project: project() });
    await screen.getByRole('button', { name: 'Add people' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add people to “Web”' });
    await dialog.getByRole('button', { name: 'Add to team' }).click();
    await expect.element(dialog.getByText('Choose people or groups.')).toBeVisible();
    expect(calls.some((call) => call.method === 'POST')).toBe(false);
  });

  it('PRJ-052: gives a role in the project without the team, then revokes a direct role', async () => {
    let current = people();
    const calls = server(MANAGER, {
      'GET /projects/5/people': () => ({ body: current }),
      'GET /users': {
        body: { items: [account(6, 'Fedor Novak', 'fedor')], next_cursor: null },
      },
      'GET /groups': { body: { items: [group(7, 'QA')], next_cursor: null } },
      'POST /role-assignments': ({ body }) => {
        const sent = body as { user_id?: number; group_id?: number };
        const holder: Assignment['holder'] =
          sent.user_id !== undefined
            ? { kind: 'user', user: ref(6, 'Fedor Novak', 'fedor') }
            : { kind: 'group', group: { id: 7, name: 'QA', builtin: null, team_of: null } };
        current = {
          ...current,
          assignments: [
            ...current.assignments,
            assignment(110 + current.assignments.length, holder, 12),
          ],
        };
        return { status: 201, body: { items: [] } };
      },
      'DELETE /role-assignments/101': () => {
        current = { ...current, assignments: current.assignments.filter((a) => a.id !== 101) };
        return { status: 204 };
      },
    });
    const screen = await render(PeoplePage, { project: project() });
    const section = screen.getByRole('region', { name: 'Other roles in the project' });

    await section.getByRole('button', { name: 'Assign role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add people to “Web”' });
    await expect
      .element(dialog.getByRole('radio', { name: 'Only a role in this project' }))
      .toBeChecked();
    await dialog.getByRole('checkbox', { name: /^Fedor Novak/ }).click();
    await dialog.getByRole('tab', { name: 'Groups' }).click();
    await dialog.getByRole('checkbox', { name: /^QA/ }).click();
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog.getByText('Choose a role.').first()).toBeVisible();
    // Roles that give nothing in a project cannot be given here.
    expect(dialog.getByRole('option', { name: 'Directory viewer' }).query()).toBeNull();
    await dialog.getByLabelText(label('Role')).selectOptions('12');
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(
      calls
        .filter((call) => call.path === '/role-assignments' && call.method === 'POST')
        .map((call) => call.body),
    ).toEqual([
      { role_id: 12, user_id: 6, global: false, projects: [5] },
      { role_id: 12, group_id: 7, global: false, projects: [5] },
    ]);
    const list = screen.getByRole('list', { name: 'Roles outside the team' });
    await expect.element(list.getByText('Fedor Novak')).toBeVisible();
    await expect.element(list.getByText('QA')).toBeVisible();

    await list.getByRole('button', { name: 'Revoke “Observer” from Dana Observer' }).click();
    const confirm = page.getByRole('dialog', { name: 'Revoke “Observer”?' });
    await expect
      .element(
        confirm.getByText('Dana Observer loses the permissions of this role in this project', {
          exact: false,
        }),
      )
      .toBeVisible();
    await confirm.getByRole('button', { name: 'Revoke' }).click();
    await expect.element(confirm).not.toBeInTheDocument();
    await expect.element(list.getByText('Dana Observer')).not.toBeInTheDocument();
    expect(toastTexts()).toEqual(expect.arrayContaining(['Role assigned', 'Role revoked']));
  });

  it('PRJ-050: removes a person and a group from the team after saying what happens', async () => {
    let current = people();
    let members = [BORIS, CLARA];
    let removals = 0;
    const calls = server(MANAGER, {
      'GET /projects/5/people': () => ({ body: current }),
      'GET /projects/5/team/users': () => ({ body: { items: members, next_cursor: null } }),
      'POST /projects/5/team/remove': ({ body }) => {
        if (++removals === 1) return failure(409, 'conflict');
        const sent = body as { users?: number[]; groups?: number[] };
        members = members.filter((member) => !(sent.users ?? []).includes(member.id));
        current = {
          ...current,
          team_groups: current.team_groups.filter((g) => !(sent.groups ?? []).includes(g.id)),
        };
        return { body: { users: sent.users ?? [], groups: sent.groups ?? [] } };
      },
    });
    const screen = await render(PeoplePage, { project: project() });

    await screen.getByRole('button', { name: 'Remove Boris Petrov from the team' }).click();
    const confirm = page.getByRole('dialog', { name: 'Remove Boris Petrov from the team?' });
    await expect
      .element(confirm.getByText('lose the team roles in this project', { exact: false }))
      .toBeVisible();
    await confirm.getByRole('button', { name: 'Remove from team' }).click();
    // An archived project answers with a conflict: it is said plainly.
    await expect
      .element(confirm.getByText('The project is archived: restore it to change its people.'))
      .toBeVisible();
    await confirm.getByRole('button', { name: 'Remove from team' }).click();
    await expect.element(confirm).not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('link', { name: 'Boris Petrov' }))
      .not.toBeInTheDocument();

    await screen.getByRole('button', { name: 'Remove “Developers” from the team' }).click();
    const groupConfirm = page.getByRole('dialog', { name: 'Remove “Developers” from the team?' });
    await expect
      .element(
        groupConfirm.getByText('Members of a group cannot leave the team one by one', {
          exact: false,
        }),
      )
      .toBeVisible();
    await groupConfirm.getByRole('button', { name: 'Remove from team' }).click();
    await expect.element(groupConfirm).not.toBeInTheDocument();
    await expect.element(screen.getByText('No groups in the team.')).toBeVisible();
    expect(
      calls.filter((call) => call.path === '/projects/5/team/remove').map((call) => call.body),
    ).toEqual([{ users: [2] }, { users: [2] }, { groups: [3] }]);
  });

  it('PRJ-053: transfers ownership after saying the previous owner keeps their roles', async () => {
    let current = people();
    const calls = server(MANAGER, {
      'GET /projects/5/people': () => ({ body: current }),
      'GET /users': {
        body: {
          items: [account(1, 'Anna Admin', 'anna'), account(2, 'Boris Petrov', 'boris')],
          next_cursor: null,
        },
      },
      'POST /projects/5/owner': () => {
        current = { ...current, owner: BORIS };
        return { body: project({ owner: BORIS, version: 2 }) };
      },
    });
    const screen = await render(PeoplePage, { project: project() });

    await screen.getByRole('button', { name: 'Transfer ownership' }).click();
    const dialog = page.getByRole('dialog', { name: 'Transfer ownership of “Web”' });
    await expect
      .element(
        dialog.getByText(
          'The new owner gets the project administrator role. The current owner keeps their roles.',
        ),
      )
      .toBeVisible();
    // The current owner is not offered.
    await expect.element(dialog.getByRole('radio', { name: /^Boris Petrov/ })).toBeVisible();
    expect(dialog.getByRole('radio', { name: /^Anna Admin/ }).query()).toBeNull();
    await dialog.getByRole('button', { name: 'Transfer ownership' }).click();
    await expect.element(dialog.getByText('Choose the new owner.').first()).toBeVisible();
    expect(calls.some((call) => call.path === '/projects/5/owner')).toBe(false);

    await dialog.getByRole('radio', { name: /^Boris Petrov/ }).click();
    await expect
      .element(
        dialog.getByText(
          'Boris Petrov becomes the owner and gets the project administrator role. Anna Admin keeps their roles.',
        ),
      )
      .toBeVisible();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Transfer ownership' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.find((call) => call.path === '/projects/5/owner')?.body).toEqual({ user_id: 2 });
    const owner = screen.getByRole('region', { name: 'Owner' });
    await expect.element(owner.getByRole('link', { name: 'Boris Petrov' })).toBeVisible();
    expect(toastTexts()).toContain('Boris Petrov owns the project now');
  });
});
