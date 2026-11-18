// The roles pages on a desktop and on a phone (step 1.1; ACC-034, ACC-044, ACC-046, ACC-050,
// ACC-052, ACC-053, ACC-054, ACC-057).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  AccessMap,
  Assignment,
  PermissionEntity,
  PermissionInfo,
  PermissionKey,
  PermissionOperation,
  PredefinedRoleKey,
  Role,
} from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import {
  failure,
  fakeApi,
  type FakeAnswer,
  type FakeRequest,
  type FakeRoute,
} from '../../../../test/fake-api.ts';
import RolePage from './RolePage.svelte';
import { ensureRole, ensureRoleAssignments, ensureRoles } from './roles.ts';
import RolesPage from './RolesPage.svelte';

// ---- The permission catalog as the server describes it ----

type Row = [PermissionKey, PermissionEntity, PermissionOperation, boolean, PermissionKey[]];

// Key, entity, operation, global only, directly implied.
const ROWS: Row[] = [
  ['system.read', 'system', 'read', true, []],
  ['system.write', 'system', 'update', true, ['system.read']],
  ['user.directory', 'user', 'read', true, []],
  ['user.details', 'user', 'read', true, ['user.directory']],
  ['user.self', 'user', 'update', true, []],
  ['user.update', 'user', 'update', true, ['user.self', 'user.details']],
  ['user.create', 'user', 'create', true, []],
  ['user.delete', 'user', 'delete', true, ['user.details']],
  ['project.create', 'project', 'create', true, []],
  ['project.basic', 'project', 'read', false, []],
  ['project.full', 'project', 'read', false, ['project.basic']],
  ['project.update', 'project', 'update', false, ['project.full']],
  ['project.delete', 'project', 'delete', false, ['project.full']],
  ['issue.read', 'issue', 'read', false, ['project.basic']],
  ['issue.create', 'issue', 'create', false, ['project.basic']],
  ['issue.update', 'issue', 'update', false, []],
  ['issue.delete', 'issue', 'delete', false, []],
  ['issue.link', 'issue', 'link', false, []],
  ['issue.private.read', 'issue', 'read', false, ['project.basic']],
  ['issue.private.update', 'issue', 'update', false, ['issue.private.read', 'issue.update']],
  ['issue.watchers.read', 'issue', 'read', false, ['project.basic']],
  ['issue.watchers.update', 'issue', 'update', false, []],
  ['issue.voters.read', 'issue', 'read', false, ['project.basic']],
  ['issue.silent', 'issue', 'update', false, []],
  ['visibility.bypass', 'visibility', 'read', false, ['issue.private.read']],
  ['comment.create', 'comment', 'create', false, []],
  ['comment.read', 'comment', 'read', false, []],
  ['comment.update.own', 'comment', 'update', false, []],
  ['comment.delete.own', 'comment', 'delete', false, []],
  ['comment.update.any', 'comment', 'update', false, ['comment.read']],
  ['comment.delete.any', 'comment', 'delete', false, ['comment.read']],
  ['attachment.create', 'attachment', 'create', false, []],
  ['attachment.update', 'attachment', 'update', false, []],
  ['attachment.delete', 'attachment', 'delete', false, []],
];

const KEYS = ROWS.map(([key]) => key);
const DIRECT = new Map(ROWS.map(([key, , , , implies]) => [key, implies]));

/** Everything `key` implies, directly or not, without itself; in catalog order. */
function implied(key: PermissionKey): PermissionKey[] {
  const found = new Set<PermissionKey>();
  const visit = (next: PermissionKey): void => {
    for (const other of DIRECT.get(next) ?? []) {
      if (!found.has(other)) {
        found.add(other);
        visit(other);
      }
    }
  };
  visit(key);
  return KEYS.filter((other) => found.has(other));
}

const CATALOG: PermissionInfo[] = ROWS.map(([key, entity, operation, global_only]) => ({
  key,
  entity,
  operation,
  global_only,
  implies: implied(key),
  dependents: KEYS.filter((other) => implied(other).includes(key)),
}));

// ---- Roles and the stand-in server ----

const EVERYTHING: AccessMap = { global: [...KEYS], projects: [], capabilities: [] };

function role(
  id: number,
  name: string,
  permissions: PermissionKey[],
  extra: Partial<Role> = {},
): Role {
  return {
    id,
    name,
    builtin: null,
    description: '',
    permissions: KEYS.filter((key) => permissions.includes(key)),
    assignments: 0,
    created_at: '2026-10-01T09:00:00Z',
    version: 1,
    ...extra,
  };
}

function predefined(
  id: number,
  builtin: PredefinedRoleKey,
  name: string,
  permissions: PermissionKey[],
  assignments = 0,
): Role {
  return role(id, name, permissions, { builtin, assignments });
}

const MEMBER: PermissionKey[] = [
  'project.basic',
  'issue.read',
  'issue.create',
  'issue.update',
  'issue.delete',
  'issue.link',
  'issue.private.read',
  'issue.private.update',
  'issue.watchers.read',
  'issue.watchers.update',
  'issue.voters.read',
  'comment.create',
  'comment.read',
  'comment.update.own',
  'comment.delete.own',
  'attachment.create',
  'attachment.update',
  'attachment.delete',
];

function predefinedRoles(): Role[] {
  return [
    predefined(1, 'system_admin', 'System administrator', KEYS, 1),
    predefined(2, 'project_admin', 'Project administrator', MEMBER, 0),
    predefined(3, 'project_member', 'Project member', MEMBER, 4),
    predefined(
      4,
      'directory_viewer',
      'Directory viewer',
      ['user.directory', 'user.details', 'user.self'],
      1,
    ),
    predefined(5, 'project_creator', 'Project creator', ['project.create']),
    predefined(6, 'account_creator', 'Account creator', ['user.create']),
  ];
}

/** A server that keeps roles, closes their permissions under implication and counts versions. */
function rolesServer(initial: Role[], assignments: Record<number, Assignment[]> = {}) {
  const roles = new Map(initial.map((item) => [item.id, structuredClone(item)]));
  let nextId = 100;
  const idOf = (request: FakeRequest): number => Number(request.path.split('/')[2]);

  function setPermission(request: FakeRequest, on: boolean): FakeAnswer {
    const found = roles.get(idOf(request));
    if (!found) return failure(404, 'not_found');
    if (found.builtin) return failure(409, 'conflict');
    const info = CATALOG.find((item) => item.key === request.path.split('/')[4]);
    if (!info) return failure(400, 'invalid_request');
    const set = new Set(found.permissions);
    if (on) for (const key of [info.key, ...info.implies]) set.add(key);
    else for (const key of [info.key, ...info.dependents]) set.delete(key);
    found.permissions = KEYS.filter((key) => set.has(key));
    found.version += 1;
    return { body: found };
  }

  const routes: Record<string, FakeRoute> = {
    'GET /roles': (request) => {
      const q = request.query.get('q')?.toLowerCase() ?? '';
      const permission = request.query.get('permission');
      const items = [...roles.values()].filter(
        (item) =>
          item.name.toLowerCase().includes(q) &&
          (!permission || item.permissions.some((key) => key === permission)),
      );
      return { body: { items, next_cursor: null } };
    },
    'POST /roles': (request) => {
      const body = request.body as { name: string; description?: string };
      const created = role(nextId++, body.name, [], { description: body.description ?? '' });
      roles.set(created.id, created);
      return { status: 201, body: created };
    },
    'GET /users/me/access': { body: EVERYTHING },
  };
  for (const id of [...roles.keys(), 100, 101]) {
    const path = `/roles/${String(id)}`;
    routes[`GET ${path}`] = (request) => {
      const found = roles.get(idOf(request));
      return found ? { body: found } : failure(404, 'not_found');
    };
    routes[`GET ${path}/assignments`] = () => ({
      body: { items: assignments[id] ?? [], next_cursor: null },
    });
    routes[`PATCH ${path}`] = (request) => {
      const found = roles.get(idOf(request));
      if (!found) return failure(404, 'not_found');
      const body = request.body as { name?: string; description?: string; version: number };
      if (body.version !== found.version) return failure(409, 'conflict');
      if (body.name !== undefined) found.name = body.name;
      if (body.description !== undefined) found.description = body.description;
      found.version += 1;
      return { body: found };
    };
    routes[`DELETE ${path}`] = (request) => {
      if (!roles.delete(idOf(request))) return failure(404, 'not_found');
      return { status: 204 };
    };
    for (const key of KEYS) {
      routes[`PUT ${path}/permissions/${key}`] = (request) => setPermission(request, true);
      routes[`DELETE ${path}/permissions/${key}`] = (request) => setPermission(request, false);
    }
  }
  return { roles, routes };
}

async function rolesList() {
  const list = await ensureRoles({ q: '', permission: '' });
  await list.refresh();
  return list;
}

async function rolePage(id: number, ondeleted = vi.fn()) {
  const [handle, holders] = await Promise.all([ensureRole(id), ensureRoleAssignments(id)]);
  await holders.refresh();
  return render(RolePage, { role: handle, holders, catalog: CATALOG, ondeleted });
}

/** A field by its label; required fields add "*" to the label text. */
function label(text: string): RegExp {
  return new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\*?$`);
}

afterEach(() => {
  vi.unstubAllGlobals();
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
  store.entities.table('role').clear();
  store.entities.table('role_assignment').clear();
});

describe('RolesPage', () => {
  it('ACC-052, ACC-053: predefined roles come first and are marked; search by name and by permission', async () => {
    setAccessForTests(EVERYTHING);
    const support = role(20, 'Support agent', ['project.basic', 'issue.read', 'comment.create'], {
      description: 'Answers customers.',
      assignments: 2,
    });
    const server = rolesServer([...predefinedRoles(), support]);
    const calls = fakeApi(server.routes);
    const onfilter = vi.fn();
    const screen = await render(RolesPage, {
      roles: await rolesList(),
      catalog: CATALOG,
      oncreated: vi.fn(),
      onfilter,
    });

    const rows = screen.getByRole('listitem');
    await expect
      .element(rows.first().getByRole('link'))
      .toHaveAccessibleName('System administrator');
    expect(rows.elements()).toHaveLength(7);
    const member = screen.getByRole('link', { name: 'Project member' });
    await expect
      .element(member)
      .toHaveAccessibleDescription(
        'Predefined. Works with issues, comments and attachments. The usual role of a project team. 18 permissions · 4 assignments',
      );
    const memberRow = rows.filter({ hasText: 'Project member' });
    await expect.element(memberRow.getByText('Predefined', { exact: true })).toBeVisible();
    const custom = screen.getByRole('link', { name: 'Support agent' });
    await expect
      .element(custom)
      .toHaveAccessibleDescription('Answers customers. 3 permissions · 2 assignments');
    await expect.element(custom).toHaveAttribute('href', '/admin/roles/20');
    const customRow = rows.filter({ hasText: 'Support agent' });
    await expect
      .element(customRow.getByText('Predefined', { exact: true }))
      .not.toBeInTheDocument();
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await screen.getByLabelText('With permission').selectOptions('project.delete');
    await expect
      .element(screen.getByRole('link', { name: 'Support agent' }))
      .not.toBeInTheDocument();
    await expect.element(screen.getByRole('link', { name: 'System administrator' })).toBeVisible();
    expect(screen.getByRole('link').elements()).toHaveLength(1);
    expect(calls.at(-1)?.query.get('permission')).toBe('project.delete');

    await screen.getByLabelText('With permission').selectOptions('');
    await screen.getByLabelText('Find roles').fill('agent');
    await expect
      .element(screen.getByRole('link', { name: 'Project member' }))
      .not.toBeInTheDocument();
    await expect.element(screen.getByRole('link', { name: 'Support agent' })).toBeVisible();
    expect(onfilter).toHaveBeenLastCalledWith({ q: 'agent', permission: '' });

    await screen.getByLabelText('Find roles').fill('nothing like it');
    await expect.element(screen.getByText('No roles found')).toBeVisible();
  });

  it('ACC-053: a person without access to roles is told whom to ask', async () => {
    setAccessForTests({ global: ['user.directory'], projects: [], capabilities: [] });
    const screen = await render(RolesPage, { roles: null, catalog: CATALOG, oncreated: vi.fn() });
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Only administrators can open this');
    await expect
      .element(screen.getByRole('button', { name: 'Create role' }))
      .not.toBeInTheDocument();
  });
});

describe('creating and editing a role', () => {
  it('ACC-054, ACC-046: creates a role, then a permission switches on what it implies and off what depends on it', async () => {
    setAccessForTests(EVERYTHING);
    const server = rolesServer(predefinedRoles());
    const calls = fakeApi(server.routes);
    const oncreated = vi.fn();
    const list = await render(RolesPage, {
      roles: await rolesList(),
      catalog: CATALOG,
      oncreated,
    });

    await list.getByRole('button', { name: 'Create role' }).click();
    const dialog = page.getByRole('dialog', { name: 'New role' });
    await dialog.getByRole('button', { name: 'Create role' }).click();
    await expect.element(dialog.getByText('Fill in this field.')).toBeVisible();
    await dialog.getByLabelText(label('Name')).fill('Support agent');
    await dialog.getByLabelText(label('Description')).fill('Answers customers.');
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Create role' }).click();
    await vi.waitFor(() => {
      expect(oncreated).toHaveBeenCalledOnce();
    });
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({
      name: 'Support agent',
      description: 'Answers customers.',
    });
    expect(toasts.items.at(-1)?.message).toBe('Role created. Choose its permissions.');
    const created = oncreated.mock.lastCall?.[0] as Role;
    await list.unmount();

    const screen = await rolePage(created.id);
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Support agent');
    await expect.element(screen.getByText('0 of 34 switched on')).toBeVisible();
    const editPrivate = screen.getByRole('checkbox', { name: 'Edit private fields' });
    await expect
      .element(editPrivate)
      .toHaveAccessibleDescription(
        /Also switches on: See projects, Edit issues, Read private fields$/,
      );
    await expect.element(editPrivate).not.toBeChecked();

    // The whole row is the target: its description toggles it too.
    await screen.getByText('Change fields hidden from most people.').click();
    await expect.element(editPrivate).toBeChecked();
    await expect
      .element(screen.getByRole('checkbox', { name: 'Read private fields' }))
      .toBeChecked();
    await expect.element(screen.getByRole('checkbox', { name: 'Edit issues' })).toBeChecked();
    await expect.element(screen.getByRole('checkbox', { name: 'See projects' })).toBeChecked();
    await expect.element(screen.getByText('4 of 34 switched on')).toBeVisible();
    expect(calls.at(-1)).toMatchObject({
      method: 'PUT',
      path: `/roles/${String(created.id)}/permissions/issue.private.update`,
    });

    const seeProjects = screen.getByRole('checkbox', { name: 'See projects' });
    await expect
      .element(seeProjects)
      .toHaveAccessibleDescription(
        /Also switches off: See project settings, .*Edit private fields/,
      );
    await seeProjects.click();
    await expect.element(seeProjects).not.toBeChecked();
    await expect.element(editPrivate).not.toBeChecked();
    await expect
      .element(screen.getByRole('checkbox', { name: 'Read private fields' }))
      .not.toBeChecked();
    await expect.element(screen.getByRole('checkbox', { name: 'Edit issues' })).toBeChecked();
    await expect.element(screen.getByText('1 of 34 switched on')).toBeVisible();
    expect(calls.at(-1)).toMatchObject({
      method: 'DELETE',
      path: `/roles/${String(created.id)}/permissions/project.basic`,
    });
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ACC-034: the catalog is grouped by area or by action and filtered by name', async () => {
    setAccessForTests(EVERYTHING);
    fakeApi(rolesServer([role(30, 'Reporter', ['issue.create'])]).routes);
    const screen = await rolePage(30);
    await expect.element(screen.getByRole('heading', { level: 3, name: 'Issues' })).toBeVisible();
    expect(screen.getByRole('checkbox').elements()).toHaveLength(34);

    await screen.getByLabelText('By action').click();
    await expect.element(screen.getByRole('heading', { level: 3, name: 'Delete' })).toBeVisible();
    await expect
      .element(screen.getByRole('heading', { level: 3, name: 'Issues' }))
      .not.toBeInTheDocument();

    await screen.getByLabelText('Find a permission').fill('comment');
    expect(screen.getByRole('checkbox').elements()).toHaveLength(6);
    await screen.getByLabelText('Find a permission').fill('zzz');
    await expect.element(screen.getByText('No permissions match “zzz”.')).toBeVisible();
  });

  it('ACC-054: name and description are saved by an explicit save, with a conflict shown in place', async () => {
    setAccessForTests(EVERYTHING);
    const server = rolesServer([role(31, 'Triage', [], { description: 'Sorts issues.' })]);
    const calls = fakeApi(server.routes);
    const screen = await rolePage(31);
    const save = screen.getByRole('button', { name: 'Save changes' });
    await expect.element(save).toBeDisabled();

    await screen.getByLabelText(label('Name')).fill('Triage team');
    await save.click();
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Triage team');
    expect(calls.at(-1)).toMatchObject({
      method: 'PATCH',
      body: { name: 'Triage team', description: 'Sorts issues.', version: 1 },
    });
    expect(toasts.items.at(-1)?.message).toBe('Role saved');

    // Someone else renames it meanwhile.
    const stored = server.roles.get(31);
    if (stored) Object.assign(stored, { name: 'Triage desk', version: stored.version + 1 });
    await screen.getByLabelText(label('Name')).fill('Triage crew');
    await save.click();
    await expect
      .element(
        screen.getByText('Someone else changed the name or description meanwhile.', {
          exact: false,
        }),
      )
      .toBeVisible();
    await expect.element(screen.getByLabelText(label('Name'))).toHaveValue('Triage crew');
    await save.click();
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Triage crew');
    await expectAccessible(screen.container);
  });

  it('ACC-054: a name a predefined role has is taken', async () => {
    setAccessForTests(EVERYTHING);
    fakeApi({
      ...rolesServer(predefinedRoles()).routes,
      'POST /roles': failure(400, 'invalid_request', [
        { path: 'name', code: 'taken', message: 'a predefined role has this name' },
      ]),
    });
    const screen = await render(RolesPage, {
      roles: await rolesList(),
      catalog: CATALOG,
      oncreated: vi.fn(),
    });
    await screen.getByRole('button', { name: 'Create role' }).click();
    const dialog = page.getByRole('dialog', { name: 'New role' });
    await dialog.getByLabelText(label('Name')).fill('Участник проекта');
    await dialog.getByRole('button', { name: 'Create role' }).click();
    await expect
      .element(dialog.getByText('Another role has this name.', { exact: false }))
      .toBeVisible();
    await expect.element(dialog.getByLabelText(label('Name'))).toHaveFocus();
  });
});

describe('RolePage', () => {
  it('ACC-044: global-only permissions are marked and explained', async () => {
    setAccessForTests(EVERYTHING);
    fakeApi(rolesServer([role(40, 'Auditor', ['system.read'])]).routes);
    const screen = await rolePage(40);
    await expect
      .element(
        screen.getByText(
          'Permissions marked “Global only” work only when the role is assigned globally, not in a project.',
        ),
      )
      .toBeVisible();
    await expect
      .element(screen.getByRole('checkbox', { name: 'Read system settings' }))
      .toHaveAccessibleDescription(/^Global only\./);
    await expect
      .element(screen.getByRole('checkbox', { name: 'Read issues' }))
      .not.toHaveAccessibleDescription(/Global only/);
    expect(screen.getByText('Global only', { exact: true }).elements()).toHaveLength(9);
  });

  it('ACC-050: a permission the person does not hold is refused, naming what is missing', async () => {
    setAccessForTests({ global: ['system.read', 'system.write'], projects: [], capabilities: [] });
    const server = rolesServer([role(50, 'Moderator', [])]);
    fakeApi({
      ...server.routes,
      'PUT /roles/50/permissions/visibility.bypass': {
        status: 403,
        body: {
          code: 'forbidden',
          message: 'missing permissions',
          missing_permissions: ['visibility.bypass', 'issue.private.read', 'project.basic'],
        },
      },
    });
    const screen = await rolePage(50);
    const bypass = screen.getByRole('checkbox', { name: 'See restricted items' });
    await bypass.click();
    await vi.waitFor(() => {
      expect(toasts.items.at(-1)?.message).toBe(
        'You can give only permissions you hold yourself. Missing: See restricted items, Read private fields, See projects.',
      );
    });
    expect(toasts.items.at(-1)?.tone).toBe('error');
    await expect.element(bypass).not.toBeChecked();
    await expect.element(screen.getByRole('checkbox', { name: 'See projects' })).not.toBeChecked();
  });

  it('ACC-052: a predefined role is read-only and shown in the viewer’s language', async () => {
    setAccessForTests(EVERYTHING);
    setLocale('ru');
    fakeApi(rolesServer(predefinedRoles()).routes);
    const screen = await rolePage(3);
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Участник проекта');
    await expect
      .element(screen.getByText('Эта роль встроена в Простор', { exact: false }))
      .toBeVisible();
    await expect.element(screen.getByText('Включено 18 из 34')).toBeVisible();
    const boxes = screen.getByRole('checkbox').elements();
    expect(boxes).toHaveLength(34);
    expect(boxes.every((box) => (box as HTMLInputElement).disabled)).toBe(true);
    await expect.element(screen.getByRole('checkbox', { name: 'Просмотр задач' })).toBeChecked();
    await expect
      .element(screen.getByRole('button', { name: 'Удалить роль' }))
      .not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'Сохранить изменения' }))
      .not.toBeInTheDocument();
    await expectAccessible(screen.container);
  });

  it('ACC-054: without the right to change roles a custom role is read-only too', async () => {
    setAccessForTests({ global: ['system.read'], projects: [], capabilities: [] });
    fakeApi(rolesServer([role(51, 'Support agent', ['issue.read'])]).routes);
    const screen = await rolePage(51);
    await expect
      .element(
        screen.getByText('Roles can be changed only with the permission “Change system settings”.'),
      )
      .toBeVisible();
    await expect.element(screen.getByRole('checkbox', { name: 'Read issues' })).toBeDisabled();
    await expect
      .element(screen.getByRole('button', { name: 'Delete role' }))
      .not.toBeInTheDocument();
  });

  it('ACC-057: deleting a role lists who loses it and where, then revokes it', async () => {
    setAccessForTests(EVERYTHING);
    const holders: Assignment[] = [
      {
        id: 7,
        role: { id: 60, name: 'Release manager', builtin: null },
        project_id: null,
        created_at: '2026-10-10T09:00:00Z',
        holder: {
          kind: 'user',
          user: {
            id: 2,
            kind: 'person',
            username: 'anna',
            full_name: 'Anna Petrova',
            avatar_color: 3,
            avatar_url: null,
            disabled: false,
            pseudonym: null,
          },
        },
      },
      {
        id: 8,
        role: { id: 60, name: 'Release manager', builtin: null },
        project_id: 3,
        created_at: '2026-10-11T09:00:00Z',
        holder: {
          kind: 'group',
          group: { id: 5, name: 'Release team', builtin: null, team_of: null },
        },
      },
    ];
    const server = rolesServer(
      [role(60, 'Release manager', ['project.update'], { assignments: 2 })],
      { 60: holders },
    );
    let attempts = 0;
    const calls = fakeApi({
      ...server.routes,
      'DELETE /roles/60': () => {
        if (++attempts === 1) return failure(409, 'last_administrator');
        server.roles.delete(60);
        return { status: 204 };
      },
    });
    const ondeleted = vi.fn();
    const screen = await rolePage(60, ondeleted);

    const anna = screen.getByRole('link', { name: /Anna Petrova/ });
    await expect.element(anna).toHaveAttribute('href', '/users/2');
    await expect.element(anna.getByText('Global', { exact: false })).toBeVisible();
    const team = screen.getByRole('link', { name: /Release team/ });
    await expect.element(team).toHaveAttribute('href', '/admin/groups/5');
    await expect.element(team.getByText('Project 3', { exact: false })).toBeVisible();
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);

    await screen.getByRole('button', { name: 'Delete role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete role “Release manager”?' });
    await expect
      .element(dialog.getByText('These people and groups lose the role', { exact: false }))
      .toBeVisible();
    await expect
      .element(dialog.getByRole('listitem').nth(0))
      .toHaveTextContent('Anna Petrova · Global');
    await expect
      .element(dialog.getByRole('listitem').nth(1))
      .toHaveTextContent('Release team (group) · Project 3');
    await expectAccessible(dialog.element());

    // The last way to administer Prostor cannot go.
    await dialog.getByRole('button', { name: 'Delete role' }).click();
    await expect
      .element(
        dialog.getByText('This would leave nobody able to administer Prostor.', {
          exact: false,
        }),
      )
      .toBeVisible();
    expect(ondeleted).not.toHaveBeenCalled();

    await dialog.getByRole('button', { name: 'Delete role' }).click();
    await vi.waitFor(() => {
      expect(ondeleted).toHaveBeenCalledOnce();
    });
    expect(calls.filter((call) => call.method === 'DELETE')).toHaveLength(2);
    expect(toasts.items.at(-1)?.message).toBe('Role deleted');
  });

  it('ACC-053: a role that does not exist is not found', async () => {
    setAccessForTests(EVERYTHING);
    const screen = await render(RolePage, {
      role: null,
      holders: null,
      catalog: CATALOG,
      ondeleted: vi.fn(),
    });
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Nothing here');
  });
});
