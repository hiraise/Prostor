// The groups pages on a desktop and on a phone (step 1.1; ACC-021, ACC-022, ACC-023, ACC-024,
// ACC-026, ACC-032, ACC-060).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  AccessMap,
  AccountSummary,
  Group,
  PermissionInfo,
  PermissionKey,
  Project,
  Role,
  Session,
  UserRef,
} from '#lib/api/schema.gen.ts';
import { permissions } from '#lib/features/access/index.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeAnswer, type FakeRoute } from '../../../../test/fake-api.ts';
import GroupPage from './GroupPage.svelte';
import { ensureGroups, openGroup } from './groups.ts';
import GroupsPage from './GroupsPage.svelte';

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

const ADMIN: AccessMap = {
  global: ['system.read', 'system.write', 'user.directory', 'user.details'],
  projects: [],
  capabilities: [],
};

const READER: AccessMap = { global: ['system.read'], projects: [], capabilities: [] };

/** Manages groups through a project, but may not assign roles globally. */
const PROJECT_ADMIN: AccessMap = {
  global: ['user.directory'],
  projects: [{ project_id: 5, permissions: ['project.update', 'project.full'] }],
  capabilities: [],
};

function group(id: number, name: string, extra: Partial<Group> = {}): Group {
  return {
    id,
    name,
    builtin: null,
    team_of: null,
    description: '',
    auto_join: false,
    members: 0,
    roles: [],
    created_at: '2026-10-20T09:00:00Z',
    version: 1,
    ...extra,
  };
}

const EVERYONE = group(1, 'Everyone', { builtin: 'everyone', auto_join: true, members: 5 });
const ALL_MEMBERS = group(2, 'All members', {
  builtin: 'all_members',
  auto_join: true,
  members: 4,
  roles: [
    {
      assignment_id: 50,
      project_id: null,
      role: { id: 4, name: 'Directory viewer', builtin: 'directory_viewer' },
    },
  ],
});
const DEVELOPERS = group(3, 'Developers', {
  description: 'People who write the code',
  auto_join: true,
  members: 1,
  version: 4,
  roles: [
    {
      assignment_id: 60,
      project_id: null,
      role: { id: 3, name: 'Project member', builtin: 'project_member' },
    },
  ],
});
const QA = group(4, 'QA');

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

/** The permission catalog with its global-only marks (ACC-044). */
const CATALOG: PermissionInfo[] = (Object.keys(permissions) as PermissionKey[]).map((key) => ({
  key,
  entity: 'project',
  operation: 'read',
  global_only: GLOBAL_ONLY.includes(key),
  implies: [],
  dependents: [],
}));

function project(id: number, name: string, key: string): Project {
  return {
    id,
    name,
    key,
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
    logo_color: id % 8,
    logo_url: null,
    next_number: 1,
    owner: null,
    team: null,
    team_size: 0,
    version: 1,
  };
}

const WEB = project(5, 'Web', 'WEB');
const MOBILE = project(6, 'Mobile', 'MOB');

/** A field by its label; required fields add "*" to the label text. */
function label(text: string): RegExp {
  return new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\*?$`);
}

/** The routes every page here needs, with `routes` added or replacing them. */
function server(access: AccessMap, routes: Record<string, FakeRoute>): ReturnType<typeof fakeApi> {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi({
    'GET /users/me/access': { body: access },
    'GET /permissions': { body: { permissions: CATALOG } },
    'GET /projects/5': { body: WEB },
    'GET /projects/6': { body: MOBILE },
    ...routes,
  });
}

function forbiddenFor(missing: string[]): FakeAnswer {
  return {
    status: 403,
    body: { code: 'forbidden', message: 'missing permissions', missing_permissions: missing },
  };
}

async function groupsPage(oncreated = vi.fn()): Promise<ReturnType<typeof render>> {
  const list = await ensureGroups();
  await list.refresh();
  return render(GroupsPage, { groups: list, oncreated });
}

async function groupPage(id: number, ondeleted = vi.fn()): Promise<ReturnType<typeof render>> {
  const opened = await openGroup(id);
  if (opened.kind !== 'found') throw new Error(`group ${String(id)} is ${opened.kind}`);
  return render(GroupPage, { group: opened.group, ondeleted });
}

afterEach(() => {
  vi.unstubAllGlobals();
  store.entities.table('group').clear();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
});

describe('GroupsPage', () => {
  it('ACC-021, ACC-022: built-in groups first, with badges, member counts and roles', async () => {
    server(ADMIN, {
      'GET /groups': {
        body: { items: [EVERYONE, ALL_MEMBERS, DEVELOPERS, QA], next_cursor: null },
      },
    });
    const screen = await groupsPage();

    const list = screen.getByRole('list', { name: 'Groups' });
    await expect.element(list.getByRole('link', { name: 'Everyone' })).toBeVisible();
    expect(
      list
        .getByRole('link')
        .elements()
        .map((link) => link.textContent.trim()),
    ).toEqual(['Everyone', 'All members', 'Developers', 'QA']);
    expect(list.getByText('Built-in', { exact: true }).elements()).toHaveLength(2);
    await expect
      .element(list.getByText('Every account except the guest.', { exact: true }))
      .toBeVisible();
    await expect.element(list.getByText('People who write the code')).toBeVisible();
    await expect.element(list.getByText('Auto-join', { exact: true })).toBeVisible();
    await expect.element(list.getByText('5 members')).toBeVisible();
    await expect.element(list.getByText('1 member', { exact: true })).toBeVisible();
    await expect.element(list.getByText('0 members')).toBeVisible();
    await expect.element(list.getByText('Project member · Global')).toBeVisible();
    await expect.element(list.getByText('Directory viewer · Global')).toBeVisible();
    // Built-in groups cannot be deleted, so they cannot be selected.
    expect(screen.getByRole('checkbox', { name: 'Select “Everyone”' }).query()).toBeNull();
    await expect
      .element(screen.getByRole('checkbox', { name: 'Select “Developers”' }))
      .toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Create group' })).toBeVisible();
    expect(screen.getByRole('link', { name: 'QA' }).element().getAttribute('href')).toMatch(
      /\/admin\/groups\/4$/,
    );
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ACC-021, ACC-022: built-in names and plurals in Russian; a reader gets no actions', async () => {
    setLocale('ru');
    server(READER, {
      'GET /groups': { body: { items: [EVERYONE, ALL_MEMBERS, DEVELOPERS], next_cursor: null } },
    });
    const screen = await groupsPage();

    await expect.element(screen.getByRole('link', { name: 'Все', exact: true })).toBeVisible();
    await expect.element(screen.getByRole('link', { name: 'Все участники' })).toBeVisible();
    await expect.element(screen.getByText('5 участников')).toBeVisible();
    await expect.element(screen.getByText('4 участника')).toBeVisible();
    await expect.element(screen.getByText('Участник проекта · Глобально')).toBeVisible();
    expect(screen.getByText('Встроенная', { exact: true }).elements()).toHaveLength(2);
    expect(screen.getByRole('button', { name: 'Создать группу' }).query()).toBeNull();
    expect(screen.getByRole('checkbox').query()).toBeNull();
    await expectAccessible(screen.container);
  });

  it('ACC-023: a taken name is reported on its field, then the group is created', async () => {
    let attempts = 0;
    const calls = server(PROJECT_ADMIN, {
      'GET /groups': { body: { items: [EVERYONE, ALL_MEMBERS, DEVELOPERS], next_cursor: null } },
      'POST /groups': ({ body }) =>
        ++attempts === 1
          ? failure(400, 'invalid_request', [{ path: 'name', code: 'taken', message: 'x' }])
          : { status: 201, body: group(9, (body as { name: string }).name, { auto_join: true }) },
    });
    const oncreated = vi.fn();
    const screen = await groupsPage(oncreated);

    await screen.getByRole('button', { name: 'Create group' }).click();
    const dialog = page.getByRole('dialog', { name: 'New group' });
    await dialog.getByRole('button', { name: 'Create group' }).click();
    await expect.element(dialog.getByText('Fill in this field.')).toBeVisible();
    expect(calls.some((call) => call.method === 'POST')).toBe(false);

    await dialog.getByLabelText(label('Name')).fill('Developers');
    await dialog.getByLabelText(label('Description')).fill('Writes the code');
    await dialog.getByText('Add new accounts automatically').click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Create group' }).click();
    await expect.element(dialog.getByText('Another group has this name.')).toBeVisible();
    await expect.element(dialog.getByLabelText(label('Name'))).toHaveFocus();
    expect(oncreated).not.toHaveBeenCalled();

    await dialog.getByLabelText(label('Name')).fill('Designers');
    await dialog.getByRole('button', { name: 'Create group' }).click();
    await vi.waitFor(() => {
      expect(oncreated).toHaveBeenCalledOnce();
    });
    expect(oncreated.mock.calls[0]?.[0]).toMatchObject({ id: 9, name: 'Designers' });
    expect(calls.filter((call) => call.method === 'POST').at(-1)?.body).toEqual({
      name: 'Designers',
      description: 'Writes the code',
      auto_join: true,
    });
    await expect.element(dialog).not.toBeInTheDocument();
  });

  it('ACC-032: shows what ends, asks for a replacement and deletes the selection', async () => {
    let groups = [EVERYONE, ALL_MEMBERS, DEVELOPERS, QA];
    let deletions = 0;
    const calls = server(ADMIN, {
      'GET /groups': () => ({ body: { items: groups, next_cursor: null } }),
      'POST /groups/deletion-impact': { body: { groups: 2, memberships: 3, assignments: 1 } },
      'POST /groups/delete': () => {
        if (++deletions === 1) return failure(409, 'last_administrator');
        groups = [EVERYONE, ALL_MEMBERS];
        return { status: 204 };
      },
    });
    const screen = await groupsPage();

    await screen.getByRole('checkbox', { name: 'Select “Developers”' }).click();
    await screen.getByRole('checkbox', { name: 'Select “QA”' }).click();
    const selection = screen.getByRole('group', { name: 'Selected groups' });
    await expect.element(selection.getByText('Selected: 2')).toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await selection.getByRole('button', { name: 'Delete groups' }).click();

    const dialog = page.getByRole('dialog', { name: 'Delete 2 groups?' });
    await expect.element(dialog.getByText('3 memberships end.')).toBeVisible();
    await expect.element(dialog.getByText('1 role assignment is revoked.')).toBeVisible();
    expect(calls.find((call) => call.path === '/groups/deletion-impact')?.body).toEqual({
      groups: [3, 4],
    });
    await expect
      .element(dialog.getByText('restricted content stays restricted', { exact: false }))
      .toBeVisible();
    // The groups being deleted are not offered as their own replacement.
    await expect.element(dialog.getByRole('radio', { name: /^All members/ })).toBeVisible();
    expect(dialog.getByRole('radio', { name: /^Developers/ }).query()).toBeNull();

    await dialog.getByRole('button', { name: 'Delete groups' }).click();
    await expect.element(dialog.getByText('Choose a replacement group.')).toBeVisible();
    expect(calls.some((call) => call.path === '/groups/delete')).toBe(false);

    await dialog.getByRole('radio', { name: /^All members/ }).click();
    await expectAccessible(dialog.element());
    // The last-administrator guard is said plainly.
    await dialog.getByRole('button', { name: 'Delete groups' }).click();
    await expect
      .element(dialog.getByText('This would leave no system administrator', { exact: false }))
      .toBeVisible();

    await dialog.getByRole('button', { name: 'Delete groups' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.filter((call) => call.path === '/groups/delete').at(-1)?.body).toEqual({
      groups: [3, 4],
      replacement: 2,
    });
    await expect.element(screen.getByRole('link', { name: 'Developers' })).not.toBeInTheDocument();
    await expect.element(screen.getByRole('link', { name: 'QA' })).not.toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Selected groups' }).query()).toBeNull();
  });

  it('ACC-022, ACC-033: a filter shows project teams, marked, opening their people page', async () => {
    const team = group(50, 'Web', {
      team_of: 5,
      members: 4,
      roles: [
        {
          assignment_id: 80,
          project_id: 5,
          role: { id: 3, name: 'Project member', builtin: 'project_member' },
        },
      ],
    });
    const calls = server(ADMIN, {
      'GET /groups': ({ query }) => {
        const kind = query.get('kind');
        const items =
          kind === 'teams'
            ? [team]
            : kind === 'all'
              ? [EVERYONE, DEVELOPERS, team]
              : [EVERYONE, DEVELOPERS];
        return { body: { items, next_cursor: null } };
      },
    });
    const onsearch = vi.fn();
    const list = await ensureGroups();
    await list.refresh();
    const screen = await render(GroupsPage, { groups: list, oncreated: vi.fn(), onsearch });

    await expect.element(screen.getByRole('link', { name: 'Developers' })).toBeVisible();
    expect(screen.getByRole('link', { name: 'Web team' }).query()).toBeNull();
    await screen.getByLabelText('Show').selectOptions('teams');
    const row = screen.getByRole('listitem').filter({ hasText: 'Web team' });
    await expect
      .element(screen.getByRole('link', { name: 'Web team' }))
      .toHaveAttribute('href', '/projects/5/settings/people');
    expect(screen.getByRole('link', { name: 'Developers' }).query()).toBeNull();
    await expect.element(row.getByText('Project team', { exact: true })).toBeVisible();
    await expect.element(row.getByText('4 members')).toBeVisible();
    await expect.element(row.getByText('Project member · Web')).toBeVisible();
    // Teams cannot be deleted here, so they cannot be selected.
    expect(screen.getByRole('checkbox', { name: 'Select “Web team”' }).query()).toBeNull();
    expect(onsearch).toHaveBeenLastCalledWith('', 'teams');
    expect(
      calls.some((call) => call.path === '/groups' && call.query.get('kind') === 'teams'),
    ).toBe(true);

    await screen.getByLabelText('Show').selectOptions('all');
    await expect.element(screen.getByRole('link', { name: 'Developers' })).toBeVisible();
    await expect.element(screen.getByRole('link', { name: 'Web team' })).toBeVisible();
    await expect
      .element(screen.getByRole('checkbox', { name: 'Select “Developers”' }))
      .toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });
});

describe('openGroup', () => {
  it('ACC-032: the address of a deleted group leads to its replacement', async () => {
    server(ADMIN, {
      'GET /groups/9': { body: ALL_MEMBERS },
      'GET /groups/8': failure(404, 'not_found'),
    });
    expect(await openGroup(9)).toEqual({ kind: 'replaced', id: 2 });
    expect(await openGroup(8)).toEqual({ kind: 'missing' });
  });
});

describe('GroupPage', () => {
  it('ACC-024: saves changed settings on the version shown, and reports a conflict', async () => {
    let current = DEVELOPERS;
    let saves = 0;
    const calls = server(ADMIN, {
      'GET /groups/3': () => ({ body: current }),
      'GET /groups/3/members': {
        body: { items: [ref(1, 'Anna Admin', 'anna')], next_cursor: null },
      },
      'PATCH /groups/3': ({ body }) => {
        if (++saves === 1) {
          current = { ...current, ...(body as Partial<Group>), version: 5 };
          return { body: current };
        }
        // Someone else changed the description first.
        current = { ...current, description: 'Their description', version: 6 };
        return failure(409, 'conflict');
      },
    });
    const screen = await groupPage(3);

    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Developers' }))
      .toBeVisible();
    const settings = screen.getByRole('region', { name: 'Settings' });
    await expect.element(settings.getByRole('button', { name: 'Save changes' })).toBeDisabled();
    await settings.getByLabelText(label('Name')).fill('Backend');
    await settings.getByLabelText(label('Description')).fill('Server code');
    await settings.getByRole('button', { name: 'Save changes' }).click();
    await expect.element(screen.getByRole('heading', { level: 1, name: 'Backend' })).toBeVisible();
    expect(calls.find((call) => call.method === 'PATCH')?.body).toEqual({
      name: 'Backend',
      description: 'Server code',
      version: 4,
    });

    await settings.getByLabelText(label('Description')).fill('Our services');
    await settings.getByRole('button', { name: 'Save changes' }).click();
    await expect
      .element(
        settings.getByText('Someone changed these settings at the same time', { exact: false }),
      )
      .toBeVisible();
    await expect
      .element(settings.getByLabelText(label('Description')))
      .toHaveValue('Their description');
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ACC-021, ACC-024: a built-in group keeps its name, auto-join and members', async () => {
    server(ADMIN, {
      'GET /groups/2': { body: ALL_MEMBERS },
      'GET /groups/2/members': {
        body: {
          items: [ref(1, 'Anna Admin', 'anna'), ref(2, 'Boris Petrov', 'boris')],
          next_cursor: null,
        },
      },
    });
    const screen = await groupPage(2);

    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'All members' }))
      .toBeVisible();
    await expect
      .element(screen.getByText('Membership is automatic', { exact: false }))
      .toBeVisible();
    await expect.element(screen.getByText('Boris Petrov')).toBeVisible();
    await expect.element(screen.getByText('You', { exact: true })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Add members' }).query()).toBeNull();
    expect(screen.getByRole('button', { name: /^Remove / }).query()).toBeNull();

    const settings = screen.getByRole('region', { name: 'Settings' });
    expect(settings.getByLabelText(label('Name')).query()).toBeNull();
    await expect
      .element(settings.getByText('A built-in group keeps its name', { exact: false }))
      .toBeVisible();
    const autoJoin = settings.getByRole('checkbox', { name: 'Add new accounts automatically' });
    await expect.element(autoJoin).toBeChecked();
    await expect.element(autoJoin).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Delete group' }).query()).toBeNull();
    await expectAccessible(screen.container);
  });

  it('ACC-026: adds members, and a refusal names the missing permissions; removes a member', async () => {
    let members = [ref(1, 'Anna Admin', 'anna')];
    let adds = 0;
    let removals = 0;
    const calls = server(PROJECT_ADMIN, {
      'GET /groups/3': () => ({ body: { ...DEVELOPERS, members: members.length } }),
      'GET /groups/3/members': () => ({ body: { items: members, next_cursor: null } }),
      'GET /users': {
        body: {
          items: [account(1, 'Anna Admin', 'anna'), account(2, 'Boris Petrov', 'boris')],
          next_cursor: null,
        },
      },
      'POST /groups/3/members': () => {
        if (++adds === 1) return forbiddenFor(['issue.delete', 'project.delete']);
        members = [...members, ref(2, 'Boris Petrov', 'boris')];
        return { body: { users: [2] } };
      },
      'POST /groups/3/members/remove': () => {
        if (++removals === 1) return failure(409, 'last_administrator');
        members = members.filter((member) => member.id !== 2);
        return { body: { users: [2] } };
      },
    });
    const screen = await groupPage(3);
    const region = screen.getByRole('region', { name: 'Members' });
    await expect.element(region.getByText('Anna Admin')).toBeVisible();
    await expect.element(region.getByText('1 member', { exact: true })).toBeVisible();

    await region.getByRole('button', { name: 'Add members' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add members to “Developers”' });
    // People already shown as members are not offered.
    await expect.element(dialog.getByRole('checkbox', { name: /^Boris Petrov/ })).toBeVisible();
    expect(dialog.getByRole('checkbox', { name: /^Anna Admin/ }).query()).toBeNull();
    await dialog.getByRole('checkbox', { name: /^Boris Petrov/ }).click();
    await dialog.getByRole('button', { name: 'Add members' }).click();
    await expect
      .element(
        dialog.getByText('To add people to this group, you need every permission it gives', {
          exact: false,
        }),
      )
      .toBeVisible();
    await expect.element(dialog.getByText('Delete issues')).toBeVisible();
    await expect.element(dialog.getByText('Delete projects')).toBeVisible();
    await expectAccessible(dialog.element());

    await dialog.getByRole('button', { name: 'Add members' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(
      calls.filter((call) => call.path === '/groups/3/members' && call.method === 'POST').at(-1)
        ?.body,
    ).toEqual({
      users: [2],
    });
    await expect.element(region.getByText('Boris Petrov')).toBeVisible();
    await expect.element(region.getByText('2 members')).toBeVisible();

    await region.getByRole('button', { name: 'Remove Boris Petrov from the group' }).click();
    const confirm = page.getByRole('dialog', { name: 'Remove from the group?' });
    await expect
      .element(
        confirm.getByText('Boris Petrov loses the roles that “Developers” gives', { exact: false }),
      )
      .toBeVisible();
    await confirm.getByRole('button', { name: 'Remove member' }).click();
    await expect
      .element(confirm.getByText('This would leave no system administrator', { exact: false }))
      .toBeVisible();
    await confirm.getByRole('button', { name: 'Remove member' }).click();
    await expect.element(confirm).not.toBeInTheDocument();
    await expect.element(region.getByText('Boris Petrov')).not.toBeInTheDocument();
    expect(calls.filter((call) => call.path === '/groups/3/members/remove').at(-1)?.body).toEqual({
      users: [2],
    });
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
  });

  it('ACC-060: assigns a role globally with its permissions shown, then revokes it', async () => {
    let current: Group = { ...DEVELOPERS, roles: [] };
    const releaseManager = role(11, 'Release manager', {
      description: 'Ships releases',
      permissions: ['issue.read', 'issue.update'],
    });
    const calls = server(ADMIN, {
      'GET /groups/3': () => ({ body: current }),
      'GET /groups/3/members': { body: { items: [], next_cursor: null } },
      'GET /roles': {
        body: {
          items: [
            role(3, 'Project member', { builtin: 'project_member', permissions: ['issue.read'] }),
            releaseManager,
          ],
          next_cursor: null,
        },
      },
      'POST /role-assignments': () => {
        current = {
          ...current,
          version: current.version + 1,
          roles: [
            {
              assignment_id: 70,
              project_id: null,
              role: { id: 11, name: 'Release manager', builtin: null },
            },
          ],
        };
        return { status: 201, body: { items: [] } };
      },
      'DELETE /role-assignments/70': () => {
        current = { ...current, version: current.version + 1, roles: [] };
        return { status: 204 };
      },
    });
    const screen = await groupPage(3);
    const region = screen.getByRole('region', { name: 'Roles' });
    await expect.element(region.getByText('No roles yet')).toBeVisible();
    await expect
      .element(
        region.getByText('where they were assigned: globally or in a project', { exact: false }),
      )
      .toBeVisible();

    await region.getByRole('button', { name: 'Assign role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Assign a role to “Developers”' });
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog.getByText('Choose a role.')).toBeVisible();
    await dialog.getByLabelText(label('Role')).selectOptions('11');
    await expect.element(dialog.getByText('Ships releases')).toBeVisible();
    const allowed = dialog.getByRole('region', { name: 'What it allows' });
    await expect.element(allowed.getByText('Read issues')).toBeVisible();
    await expect.element(allowed.getByText('Edit issues')).toBeVisible();
    // Without managing projects, the role can only be assigned globally.
    await expect
      .element(dialog.getByText('In the whole system and in every project.'))
      .toBeVisible();
    expect(dialog.getByRole('radio', { name: 'In chosen projects' }).query()).toBeNull();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.find((call) => call.path === '/role-assignments')?.body).toEqual({
      role_id: 11,
      group_id: 3,
      global: true,
      projects: [],
    });
    await expect.element(region.getByText('Release manager')).toBeVisible();
    await expect.element(region.getByText('Global', { exact: true })).toBeVisible();

    await region.getByRole('button', { name: 'Revoke “Release manager”' }).click();
    const confirm = page.getByRole('dialog', { name: 'Revoke “Release manager”?' });
    await expect
      .element(
        confirm.getByText('Members of “Developers” lose the permissions of this role', {
          exact: false,
        }),
      )
      .toBeVisible();
    await confirm.getByRole('button', { name: 'Revoke role' }).click();
    await expect.element(confirm).not.toBeInTheDocument();
    expect(
      calls.some((call) => call.method === 'DELETE' && call.path === '/role-assignments/70'),
    ).toBe(true);
    await expect.element(region.getByText('No roles yet')).toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ACC-060: assigning roles globally needs system settings rights; projects need managing them', async () => {
    let current = DEVELOPERS;
    let attempts = 0;
    const calls = server(PROJECT_ADMIN, {
      'GET /groups/3': () => ({ body: current }),
      'GET /groups/3/members': { body: { items: [], next_cursor: null } },
      'GET /projects': { body: { items: [WEB, MOBILE], next_cursor: null } },
      'GET /roles': {
        body: {
          items: [
            role(3, 'Project member', { builtin: 'project_member', permissions: ['issue.read'] }),
            role(4, 'Directory viewer', {
              builtin: 'directory_viewer',
              permissions: ['user.directory', 'user.self'],
            }),
            role(11, 'Release manager', { permissions: ['issue.read', 'issue.delete'] }),
          ],
          next_cursor: null,
        },
      },
      'POST /role-assignments': () => {
        if (++attempts === 1) return forbiddenFor(['issue.delete']);
        current = {
          ...current,
          version: current.version + 1,
          roles: [
            ...current.roles,
            {
              assignment_id: 71,
              project_id: 5,
              role: { id: 11, name: 'Release manager', builtin: null },
            },
          ],
        };
        return { status: 201, body: { items: [] } };
      },
    });
    const screen = await groupPage(3);
    const region = screen.getByRole('region', { name: 'Roles' });
    await expect.element(region.getByText('Project member')).toBeVisible();
    // A global role is revoked only by people who change system settings.
    expect(region.getByRole('button', { name: 'Revoke “Project member”' }).query()).toBeNull();
    // Managing groups through a project is enough for members and settings.
    await expect
      .element(
        screen
          .getByRole('region', { name: 'Members' })
          .getByRole('button', { name: 'Add members' }),
      )
      .toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Delete group' })).toBeVisible();

    await region.getByRole('button', { name: 'Assign role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Assign a role to “Developers”' });
    await expect
      .element(dialog.getByText('Only in projects you manage', { exact: false }))
      .toBeVisible();
    // Only the managed project is offered, and only roles that work in a project.
    await expect.element(dialog.getByRole('checkbox', { name: /^Web/ })).toBeVisible();
    expect(dialog.getByRole('checkbox', { name: /^Mobile/ }).query()).toBeNull();
    expect(dialog.getByRole('option', { name: 'Directory viewer' }).query()).toBeNull();
    await dialog.getByLabelText(label('Role')).selectOptions('11');
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog.getByText('Choose at least one project.')).toBeVisible();
    expect(calls.some((call) => call.path === '/role-assignments')).toBe(false);

    await dialog.getByRole('checkbox', { name: /^Web/ }).click();
    await expect.element(dialog.getByRole('group', { name: /Chosen: 1/ })).toBeVisible();
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect
      .element(
        dialog.getByText('You can pass on only permissions you hold yourself', { exact: false }),
      )
      .toBeVisible();
    await expect
      .element(dialog.getByRole('listitem').filter({ hasText: 'Delete issues' }).first())
      .toBeVisible();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.filter((call) => call.path === '/role-assignments').at(-1)?.body).toEqual({
      role_id: 11,
      group_id: 3,
      global: false,
      projects: [5],
    });
    const list = region.getByRole('list', { name: 'Roles of “Developers”' });
    await expect.element(list.getByText('Release manager')).toBeVisible();
    await expect
      .element(list.getByRole('link', { name: 'Web' }))
      .toHaveAttribute('href', '/projects/WEB');
    // Roles in a project one manages are revoked there.
    await expect
      .element(region.getByRole('button', { name: 'Revoke “Release manager”' }))
      .toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
  });

  it('ACC-033: a project team holds roles only in its project and changes nowhere here', async () => {
    const team = group(50, 'Web', {
      team_of: 5,
      members: 4,
      roles: [
        {
          assignment_id: 80,
          project_id: 5,
          role: { id: 3, name: 'Project member', builtin: 'project_member' },
        },
      ],
    });
    const calls = server(
      { ...ADMIN, global: [...ADMIN.global, 'project.update'] },
      {
        'GET /groups/50': { body: team },
        'GET /groups/50/members': { body: { items: [], next_cursor: null } },
        'GET /roles': {
          body: {
            items: [role(11, 'Release manager', { permissions: ['issue.read'] })],
            next_cursor: null,
          },
        },
        'POST /role-assignments': { status: 201, body: { items: [] } },
      },
    );
    const screen = await groupPage(50);

    await expect.element(screen.getByRole('heading', { level: 1, name: 'Web team' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Add members' }).query()).toBeNull();
    expect(screen.getByRole('button', { name: 'Delete group' }).query()).toBeNull();
    const region = screen.getByRole('region', { name: 'Roles' });
    await region.getByRole('button', { name: 'Assign role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Assign a role to “Web team”' });
    const where = dialog.getByRole('region', { name: 'Where it applies' });
    await expect.element(where.getByText(/^Only in its project/)).toBeVisible();
    await expect.element(where.getByText('Web', { exact: true })).toBeVisible();
    expect(dialog.getByRole('radio').query()).toBeNull();
    await dialog.getByLabelText(label('Role')).selectOptions('11');
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.find((call) => call.path === '/role-assignments')?.body).toEqual({
      role_id: 11,
      group_id: 50,
      global: false,
      projects: [5],
    });
  });
});
