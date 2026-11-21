// The user directory and profiles as administrators use them, on a desktop and on a phone
// (ACC-002…ACC-006, ACC-009, ACC-015, ACC-018, ACC-026, ACC-059, ACC-061, AUTH-010, AUTH-014,
// UI-010).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  AccessMap,
  Account,
  AccountSummary,
  DisableRequest,
  EffectiveRole,
  Group,
  PermissionInfo,
  PermissionKey,
  Project,
  ProjectRef,
  Role,
  Session,
  UserDeletion,
} from '#lib/api/schema.gen.ts';
import { permissions } from '#lib/features/access/index.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import type { EntityHandle, ListHandle } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi } from '../../../../test/fake-api.ts';
import CreateAccountDialog from './CreateAccountDialog.svelte';
import UserProfilePage from './UserProfilePage.svelte';
import UsersPage from './UsersPage.svelte';
import { ensureDirectory, openProfile, type DirectoryAccount } from './users.ts';

const SESSION: Session = {
  user: {
    id: 1,
    username: 'admin',
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

function access(global: PermissionKey[]): AccessMap {
  return { global, projects: [], capabilities: [] };
}

const ADMIN = access([
  'system.read',
  'system.write',
  'user.directory',
  'user.details',
  'user.self',
  'user.update',
  'user.create',
  'user.delete',
  'project.update',
]);

// Every fixture is newer than the ones before, so rows the store kept from an earlier test never
// hide it (the store ignores older versions).
let version = 100;

function person(id: number, fullName: string, extra: Partial<AccountSummary> = {}): AccountSummary {
  const username = (fullName.split(' ')[0] ?? 'user').toLowerCase();
  return {
    id,
    full_name: fullName,
    username,
    email: `${username}@example.com`,
    email_verified: true,
    avatar_color: id % 8,
    avatar_url: null,
    created_at: '2026-10-01T09:00:00Z',
    disabled: null,
    full_profile: true,
    groups: [],
    projects: [],
    guest: false,
    invited: false,
    version: ++version,
    ...extra,
  };
}

function profile(summary: AccountSummary, tier: Account['tier']): Account {
  return {
    ...summary,
    tier,
    password: tier === 'full' ? { has_password: true, must_change: false } : null,
  };
}

function group(id: number, name: string, builtin: Group['builtin'] = null): Group {
  return {
    id,
    name,
    builtin,
    team_of: null,
    auto_join: false,
    created_at: '2026-10-01T09:00:00Z',
    description: '',
    members: 3,
    roles: [],
    version: 1,
  };
}

function role(id: number, name: string, builtin: Role['builtin'] = null): Role {
  return {
    id,
    name,
    builtin,
    assignments: 0,
    created_at: '2026-10-01T09:00:00Z',
    description: '',
    permissions: ['issue.read'],
    version: 1,
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

function projectRef(id: number, name: string, key: string): ProjectRef {
  return { id, name, key, logo_color: id % 8, logo_url: null, archived: false };
}

function project(ref: ProjectRef): Project {
  return {
    id: ref.id,
    name: ref.name,
    key: ref.key,
    kind: 'standard',
    template: 'blank',
    description: '',
    description_html: '',
    archived_at: null,
    created_at: '2026-10-01T09:00:00Z',
    default_audience: null,
    deletion: null,
    favorite: false,
    former_keys: [],
    logo_color: ref.logo_color,
    logo_url: ref.logo_url,
    next_number: 1,
    owner: null,
    team: null,
    team_size: 0,
    version: 1,
  };
}

const WEB = projectRef(5, 'Web', 'WEB');
const MOBILE = projectRef(6, 'Mobile', 'MOB');

/** The directory's first page, loaded afresh for this test. */
async function directory(): Promise<ListHandle<DirectoryAccount>> {
  const list = await ensureDirectory();
  await list.refresh();
  return list;
}

async function profileOf(id: number): Promise<EntityHandle<DirectoryAccount>> {
  const found = await openProfile(id);
  if (!found || !('handle' in found)) throw new Error(`no profile ${String(id)}`);
  return found.handle;
}

/** A field by its label; required fields add "*" to the label text. */
function label(text: string): RegExp {
  return new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\*?$`);
}

/** Runs an action on the selected rows: buttons on a desktop, a menu on a phone. */
async function bulkAction(name: string): Promise<void> {
  if (isPhone()) {
    await page.getByRole('button', { name: 'Actions', exact: true }).click();
    await page.getByRole('menuitem', { name, exact: true }).click();
  } else {
    await page
      .getByRole('group', { name: 'Actions with the selected accounts' })
      .getByRole('button', { name, exact: true })
      .click();
  }
}

/**
 * Buttons and links smaller than a touch target. A person's name in a directory row is an inline
 * link whose target is the whole row, so on a desktop it is left out.
 */
async function tooSmall(root: Element, names: string[] = []): Promise<string[]> {
  return isPhone()
    ? smallTargets(root, 44)
    : smallTargets(root, 32).then((small) => small.filter((name) => !names.includes(name)));
}

function toastTexts(): string[] {
  return toasts.items.map((toast) => toast.message);
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('UsersPage', () => {
  it('ACC-002: lists accounts with what sets them apart, and filters by state and text', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    const everyone = [
      person(1, 'Anna Admin', {
        groups: [{ id: 5, name: 'Developers', builtin: null, team_of: null }],
      }),
      person(102, 'Boris Gone', {
        disabled: { at: '2026-10-10T09:00:00Z', label: 'left', reason: 'Moved on' },
      }),
      person(103, 'Vera New', { invited: true, email_verified: false }),
      person(104, 'Guest', { guest: true, email: null }),
    ];
    const calls = fakeApi({
      'GET /users': ({ query }) => {
        const state = query.get('state');
        const text = (query.get('q') ?? '').toLowerCase();
        const items = everyone
          .filter((account) =>
            state === 'disabled'
              ? account.disabled
              : state === 'enabled'
                ? !account.disabled
                : true,
          )
          .filter((account) => account.full_name.toLowerCase().startsWith(text));
        return { body: { items, next_cursor: null } };
      },
    });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: true });

    await expect.element(screen.getByRole('heading', { level: 1, name: 'Users' })).toBeVisible();
    await expect
      .element(screen.getByRole('link', { name: 'Vera New' }))
      .toHaveAttribute('href', '/users/103');
    await expect.element(screen.getByText('You', { exact: true })).toBeVisible();
    const rows = screen.getByRole('listitem');
    await expect.element(rows.getByText('Left the organization')).toBeVisible();
    await expect.element(screen.getByText('Invited', { exact: true })).toBeVisible();
    await expect.element(screen.getByText('Guest', { exact: true }).last()).toBeVisible();
    await expect
      .element(screen.getByRole('list', { name: 'Groups of Anna Admin' }))
      .toHaveTextContent('Developers');
    await expect.element(screen.getByText('vera@example.com')).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Create account' })).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Invite people' })).toBeVisible();
    const names = everyone.map((account) => account.full_name);
    expect(await tooSmall(screen.container, names)).toEqual([]);
    await expectAccessible(screen.container);

    await screen.getByLabelText('Show', { exact: true }).selectOptions('disabled');
    await expect.element(screen.getByRole('link', { name: 'Vera New' })).not.toBeInTheDocument();
    await expect.element(screen.getByRole('link', { name: 'Boris Gone' })).toBeVisible();
    expect(calls.at(-1)?.query.get('state')).toBe('disabled');

    await screen.getByLabelText('Show', { exact: true }).selectOptions('all');
    await screen.getByRole('searchbox', { name: label('Find people') }).fill('ver');
    await expect.element(screen.getByRole('link', { name: 'Boris Gone' })).not.toBeInTheDocument();
    await expect.element(screen.getByRole('link', { name: 'Vera New' })).toBeVisible();
    expect(calls.at(-1)?.query.get('q')).toBe('ver');

    await screen.getByRole('searchbox', { name: label('Find people') }).fill('nobody');
    await expect.element(screen.getByRole('heading', { name: 'Nobody found' })).toBeVisible();
  });

  it('ACC-002: rows show the projects of each account, a few on phones, each opening it', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    const projects = [
      WEB,
      MOBILE,
      projectRef(7, 'Docs', 'DOC'),
      projectRef(8, 'Data', 'DAT'),
      projectRef(9, 'Ops', 'OPS'),
    ];
    fakeApi({
      'GET /users': {
        body: {
          items: [
            person(201, 'Yuri Teams', {
              groups: [
                { id: 5, name: 'Developers', builtin: null, team_of: null },
                { id: 50, name: 'Web', builtin: null, team_of: 5 },
              ],
              projects,
            }),
          ],
          next_cursor: null,
        },
      },
    });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: true });

    const list = screen.getByRole('list', { name: 'Projects of Yuri Teams' });
    await expect
      .element(list.getByRole('link', { name: 'Web' }))
      .toHaveAttribute('href', '/projects/WEB');
    if (isPhone()) {
      expect(list.getByRole('link').elements()).toHaveLength(2);
      await expect.element(list.getByText('and 3 more')).toBeVisible();
    } else {
      expect(list.getByRole('link').elements()).toHaveLength(4);
      await expect.element(list.getByText('and 1 more')).toBeVisible();
    }
    // Teams are shown as projects, not as groups.
    await expect
      .element(screen.getByRole('list', { name: 'Groups of Yuri Teams' }))
      .toHaveTextContent('Developers');
    expect(
      screen.getByRole('list', { name: 'Groups of Yuri Teams' }).getByText('Web team').query(),
    ).toBeNull();
    expect(await tooSmall(screen.container, ['Yuri Teams'])).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ACC-002: people who only see the directory get no actions, in Russian too', async () => {
    setLocale('ru');
    setSessionForTests(SESSION);
    setAccessForTests(access(['user.directory']));
    fakeApi({
      'GET /users': {
        body: {
          items: [
            person(111, 'Boris Gone', {
              disabled: { at: '2026-10-10T09:00:00Z', label: 'disabled', reason: null },
            }),
          ],
          next_cursor: null,
        },
      },
    });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: true });
    await expect.element(screen.getByRole('heading', { name: 'Пользователи' })).toBeVisible();
    await expect.element(screen.getByRole('listitem').getByText('Отключена')).toBeVisible();
    expect(screen.getByRole('checkbox').elements()).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Создать учётную запись' }).elements()).toHaveLength(
      0,
    );
    await expectAccessible(screen.container);
  });

  it('ACC-002: without the right to see people the page says whom to ask', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(access(['user.self']));
    const calls = fakeApi({});
    const screen = await render(UsersPage, { directory: null, emailEnabled: true });
    await expect
      .element(screen.getByRole('heading', { name: 'Only administrators can open this' }))
      .toBeVisible();
    expect(calls).toEqual([]);
  });

  it('ACC-004: invites people by email and names the addresses that failed', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    let attempts = 0;
    const calls = fakeApi({
      'GET /users': { body: { items: [], next_cursor: null } },
      'POST /users/invitations': () =>
        ++attempts === 1
          ? failure(400, 'invalid_request', [
              { path: 'emails', code: 'invalid', message: 'nope@: not an email address' },
              { path: 'emails', code: 'taken', message: 'anna@example.com: an account has it' },
            ])
          : { status: 201, body: { items: [person(301, 'Kim Lee'), person(302, 'Lee Kim')] } },
    });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: true });

    await screen.getByRole('button', { name: 'Invite people' }).click();
    const dialog = page.getByRole('dialog', { name: 'Invite people' });
    const field = dialog.getByLabelText(label('Email addresses'));
    await field.fill('nope@, anna@example.com');
    await dialog.getByRole('button', { name: 'Send invitations' }).click();
    await expect
      .element(dialog.getByText('These addresses are not valid: nope@.', { exact: false }))
      .toBeVisible();
    await expect
      .element(
        dialog.getByText('These addresses already have accounts: anna@example.com.', {
          exact: false,
        }),
      )
      .toBeVisible();
    await expect.element(field).toHaveAttribute('aria-invalid', 'true');
    await expectAccessible(dialog.element());

    await field.fill('kim@example.com\nlee@example.com');
    await dialog.getByRole('button', { name: 'Send invitations' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.at(-2)?.body).toEqual({ emails: 'kim@example.com\nlee@example.com' });
    expect(toastTexts()).toContain('2 invitations sent');
  });

  it('ACC-004: without outgoing email the dialog explains why nothing can be sent', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    fakeApi({ 'GET /users': { body: { items: [], next_cursor: null } } });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: false });
    await screen.getByRole('button', { name: 'Invite people' }).click();
    const dialog = page.getByRole('dialog', { name: 'Invite people' });
    await expect
      .element(dialog.getByText('Email is not set up on this server', { exact: false }))
      .toBeVisible();
    await expect.element(dialog.getByRole('button', { name: 'Send invitations' })).toBeDisabled();
  });

  it('ACC-005: disables the selected accounts with a status and a reason, then enables one', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    let rows = [person(401, 'Olga One'), person(402, 'Pavel Two'), person(403, 'Rita Three')];
    const calls = fakeApi({
      'GET /users': () => ({ body: { items: rows, next_cursor: null } }),
      'POST /users/disable': ({ body }) => {
        const { users, label: word, reason } = body as DisableRequest;
        rows = rows.map((row) =>
          users.includes(row.id)
            ? {
                ...row,
                version: ++version,
                disabled: { at: '2026-10-22T10:00:00Z', label: word, reason: reason ?? null },
              }
            : row,
        );
        return { status: 204 };
      },
      'POST /users/enable': ({ body }) => {
        const { users } = body as { users: number[] };
        rows = rows.map((row) =>
          users.includes(row.id) ? { ...row, version: ++version, disabled: null } : row,
        );
        return { status: 204 };
      },
    });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: true });

    await screen.getByLabelText('Select Olga One').click();
    await screen.getByLabelText('Select Rita Three').click();
    await expect.element(screen.getByText('2 selected')).toBeVisible();
    expect(await tooSmall(screen.container, ['Olga One', 'Pavel Two', 'Rita Three'])).toEqual([]);
    await bulkAction('Disable');
    const dialog = page.getByRole('dialog', { name: 'Disable 2 accounts?' });
    await dialog.getByLabelText('Status shown').selectOptions('left');
    await dialog.getByLabelText(label('Reason')).fill('Contract ended');
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Disable accounts' }).click();

    const items = screen.getByRole('listitem');
    await expect.element(items.getByText('Left the organization').first()).toBeVisible();
    expect(items.getByText('Left the organization').elements()).toHaveLength(2);
    expect(calls.find((call) => call.path === '/users/disable')?.body).toEqual({
      users: [401, 403],
      label: 'left',
      reason: 'Contract ended',
    });
    await expect.element(screen.getByText('2 selected')).not.toBeInTheDocument();
    expect(toastTexts()).toContain('2 accounts disabled');

    await screen.getByLabelText('Select Rita Three').click();
    await bulkAction('Enable');
    const confirm = page.getByRole('dialog', { name: 'Enable the account of Rita Three?' });
    await confirm.getByRole('button', { name: 'Enable account' }).click();
    await expect.element(confirm).not.toBeInTheDocument();
    await vi.waitFor(() => {
      expect(items.getByText('Left the organization').elements()).toHaveLength(1);
    });
    expect(calls.find((call) => call.path === '/users/enable')?.body).toEqual({ users: [403] });
    await expectAccessible(screen.container);
  });

  it('ACC-005: refusing to disable one’s own account is explained in the dialog', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    fakeApi({
      'GET /users': { body: { items: [person(1, 'Anna Admin')], next_cursor: null } },
      'POST /users/disable': failure(409, 'conflict'),
    });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: true });
    await screen.getByLabelText('Select Anna Admin').click();
    await bulkAction('Disable');
    const dialog = page.getByRole('dialog', { name: 'Disable the account of Anna Admin?' });
    await dialog.getByRole('button', { name: 'Disable account' }).click();
    await expect
      .element(
        dialog.getByText('Your own account and the guest cannot be disabled', { exact: false }),
      )
      .toBeVisible();
  });

  it('ACC-006: shows what goes with an account first and requires a replacement', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    let rows = [person(501, 'Sam Leaving'), person(502, 'Tom Staying')];
    const calls = fakeApi({
      'GET /users': () => ({ body: { items: rows, next_cursor: null } }),
      'POST /users/deletion-impact': {
        body: { accounts: 1, sessions: 2, tokens: 1, memberships: 3, assignments: 0 },
      },
      'POST /users/delete': ({ body }) => {
        const { users } = body as UserDeletion;
        rows = rows.filter((row) => !users.includes(row.id));
        return { status: 204 };
      },
    });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: true });

    await screen.getByLabelText('Select Sam Leaving').click();
    await bulkAction('Delete');
    const dialog = page.getByRole('dialog', { name: 'Delete the account of Sam Leaving?' });
    await expect.element(dialog.getByText('2 sessions')).toBeVisible();
    await expect.element(dialog.getByText('1 API token')).toBeVisible();
    await expect.element(dialog.getByText('3 group memberships')).toBeVisible();
    expect(dialog.getByText('roles given directly').elements()).toHaveLength(0);
    expect(calls.find((call) => call.path === '/users/deletion-impact')?.body).toEqual({
      users: [501],
    });

    // The account being deleted is never offered as its own replacement.
    await expect.element(dialog.getByRole('radio', { name: /Tom Staying/ })).toBeVisible();
    expect(dialog.getByRole('radio', { name: /Sam Leaving/ }).elements()).toHaveLength(0);
    await dialog.getByRole('button', { name: 'Delete account' }).click();
    await expect.element(dialog.getByText('Choose who takes their place.')).toBeVisible();
    expect(calls.some((call) => call.path === '/users/delete')).toBe(false);
    await expectAccessible(dialog.element());

    await dialog.getByRole('radio', { name: /Tom Staying/ }).click();
    await dialog.getByRole('button', { name: 'Delete account' }).click();
    await expect.element(screen.getByRole('link', { name: 'Sam Leaving' })).not.toBeInTheDocument();
    expect(calls.find((call) => call.path === '/users/delete')?.body).toEqual({
      users: [501],
      replacement: 502,
    });
    expect(toastTexts()).toContain('Account deleted');
  });

  it('ACC-026: adds the selected accounts to a group found by name', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    const calls = fakeApi({
      'GET /users': {
        body: { items: [person(601, 'Uma One'), person(602, 'Ivan Two')], next_cursor: null },
      },
      'GET /groups': ({ query }) => ({
        body: {
          items: [
            group(1, 'Everyone', 'everyone'),
            group(9, 'Designers'),
            group(10, 'Devs'),
          ].filter((item) =>
            item.name.toLowerCase().startsWith((query.get('q') ?? '').toLowerCase()),
          ),
          next_cursor: null,
        },
      }),
      'POST /groups/9/members': { body: { users: [601, 602] } },
    });
    const screen = await render(UsersPage, { directory: await directory(), emailEnabled: true });

    await screen.getByLabelText('Select all shown').click();
    await expect.element(screen.getByText('2 selected')).toBeVisible();
    await bulkAction('Add to group');
    const dialog = page.getByRole('dialog', { name: 'Add 2 people to a group' });
    // Built-in groups include everyone by themselves: they are not offered.
    await expect.element(dialog.getByRole('radio', { name: /Designers/ })).toBeVisible();
    expect(dialog.getByRole('radio', { name: /Everyone/ }).elements()).toHaveLength(0);
    await dialog.getByRole('searchbox', { name: label('Find a group') }).fill('dev');
    await expect.element(dialog.getByRole('radio', { name: /Designers/ })).not.toBeInTheDocument();
    await dialog.getByRole('searchbox', { name: label('Find a group') }).fill('des');
    await dialog.getByRole('radio', { name: /Designers/ }).click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Add to group' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({ users: [601, 602] });
    expect(toastTexts()).toContain('Added to “Designers”: 2');
  });
});

describe('CreateAccountDialog', () => {
  it('ACC-003, AUTH-010: checks the form, shows taken values by field, then creates', async () => {
    let attempts = 0;
    const calls = fakeApi({
      'POST /users': () =>
        ++attempts === 1
          ? failure(400, 'invalid_request', [
              { path: 'username', code: 'taken', message: 'x' },
              { path: 'email', code: 'taken', message: 'x' },
            ])
          : { status: 201, body: profile(person(201, 'Dina Dev'), 'full') },
    });
    const oncreated = vi.fn();
    await render(CreateAccountDialog, { open: true, oncreated });
    const dialog = page.getByRole('dialog', { name: 'New account' });

    await dialog.getByLabelText(label('Full name')).fill('Dina Dev');
    await dialog.getByLabelText(label('Email')).fill('Dina.Dev+work@example.com');
    // The username follows the email until it is edited.
    await expect.element(dialog.getByLabelText(label('Username'))).toHaveValue('dina.dev');
    await dialog.getByLabelText(label('Password')).fill('short');
    await dialog.getByRole('button', { name: 'Create account' }).click();
    await expect.element(dialog.getByText('Use at least 8 characters.')).toBeVisible();
    expect(calls).toEqual([]);

    await dialog.getByLabelText(label('Password')).fill('long enough pass');
    await dialog.getByLabelText(label('Repeat the password')).fill('long enough pass');
    await dialog.getByRole('button', { name: 'Create account' }).click();
    await expect.element(dialog.getByText('Someone already has this username.')).toBeVisible();
    await expect.element(dialog.getByText('Another account has this email.')).toBeVisible();
    await expect.element(dialog.getByLabelText(label('Email'))).toHaveFocus();
    await expectAccessible(dialog.element());
    expect(await smallTargets(dialog.element(), isPhone() ? 44 : 32)).toEqual([]);

    await dialog.getByLabelText(label('Username')).fill('dina');
    await dialog.getByLabelText(label('Email')).fill('dina@example.com');
    await expect.element(dialog.getByLabelText(label('Username'))).toHaveValue('dina');
    await dialog.getByRole('button', { name: 'Create account' }).click();
    await vi.waitFor(() => {
      expect(oncreated).toHaveBeenCalledOnce();
    });
    expect(calls.at(-1)?.body).toEqual({
      full_name: 'Dina Dev',
      email: 'dina@example.com',
      username: 'dina',
      password: 'long enough pass',
      must_change_password: true,
    });
  });
});

describe('UserProfilePage', () => {
  it('ACC-015, UI-010: the basic tier shows only the name block', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(access(['user.directory']));
    fakeApi({
      'GET /users/701': { body: profile(person(701, 'Uma Basic', { email: null }), 'basic') },
    });
    const screen = await render(UserProfilePage, {
      profile: await profileOf(701),
      emailEnabled: true,
    });

    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Uma Basic' }))
      .toBeVisible();
    await expect
      .element(screen.getByText('You can see only the name of this person.'))
      .toBeVisible();
    expect(screen.getByRole('heading', { name: 'Profile' }).elements()).toHaveLength(0);
    expect(screen.getByRole('heading', { name: 'Groups' }).elements()).toHaveLength(0);
    expect(screen.getByRole('heading', { name: 'Roles' }).elements()).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Actions' }).elements()).toHaveLength(0);
    await expectAccessible(screen.container);
  });

  it('ACC-015, ACC-018: the details tier shows the profile, groups and roles read-only', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(access(['user.directory', 'user.details', 'system.read']));
    const calls = fakeApi({
      'GET /users/702': {
        body: profile(
          person(702, 'Vlad Detail', {
            groups: [{ id: 7, name: 'QA', builtin: null, team_of: null }],
          }),
          'details',
        ),
      },
      'GET /users/702/roles': { body: { items: [] } },
    });
    const screen = await render(UserProfilePage, {
      profile: await profileOf(702),
      emailEnabled: true,
    });

    await expect.element(screen.getByRole('heading', { name: 'Profile' })).toBeVisible();
    await expect.element(screen.getByText('vlad@example.com')).toBeVisible();
    await expect.element(screen.getByText('Confirmed')).toBeVisible();
    const groups = screen.getByRole('region', { name: 'Groups' });
    await expect
      .element(groups.getByRole('link', { name: 'QA' }))
      .toHaveAttribute('href', '/admin/groups/7');
    await expect.element(groups.getByText('Everyone')).toBeVisible();
    await expect.element(groups.getByText('All members')).toBeVisible();
    await expect.element(screen.getByText('No roles yet.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save profile' }).elements()).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Add to group' }).elements()).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Actions' }).elements()).toHaveLength(0);
    expect(calls.some((call) => call.path === '/users/702/roles')).toBe(true);
    expect(await tooSmall(screen.container)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ACC-009, AUTH-010, AUTH-014: edits another profile and acts on the account', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    let current = profile(person(801, 'Wade Edit', { email_verified: false }), 'full');
    let attempts = 0;
    const calls = fakeApi({
      'GET /users/801': () => ({ body: current }),
      'GET /users/801/roles': { body: { items: [] } },
      'PATCH /users/801': ({ body }) => {
        if (++attempts === 1) {
          return failure(400, 'invalid_request', [
            { path: 'username', code: 'taken', message: 'x' },
          ]);
        }
        const patch = body as { full_name: string | null; username: string | null };
        current = {
          ...current,
          full_name: patch.full_name ?? current.full_name,
          username: patch.username ?? current.username,
          version: ++version,
        };
        return { body: current };
      },
      'POST /users/801/password-change': () => {
        current = {
          ...current,
          password: { has_password: true, must_change: true },
          version: ++version,
        };
        return { status: 204 };
      },
      'POST /users/801/email-verification': { status: 204 },
    });
    const screen = await render(UserProfilePage, {
      profile: await profileOf(801),
      emailEnabled: true,
    });

    await expect.element(screen.getByText('Not confirmed')).toBeVisible();
    await screen.getByLabelText(label('Full name')).fill('Wade Edited');
    await screen.getByLabelText(label('Username')).fill('taken');
    await screen.getByRole('button', { name: 'Save profile' }).click();
    await expect.element(screen.getByText('Someone already has this username.')).toBeVisible();
    await screen.getByLabelText(label('Username')).fill('wade.e');
    await screen.getByRole('button', { name: 'Save profile' }).click();
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Wade Edited' }))
      .toBeVisible();
    const sent = calls.filter((call) => call.method === 'PATCH').at(-1)?.body;
    expect(sent).toMatchObject({ full_name: 'Wade Edited', username: 'wade.e', email: null });
    expect(toastTexts()).toContain('Profile saved');

    // AUTH-014: a confirmation link for the unconfirmed address.
    await screen.getByRole('button', { name: 'Actions', exact: true }).click();
    await page.getByRole('menuitem', { name: 'Send confirmation link' }).click();
    await vi.waitFor(() => {
      expect(toastTexts()).toContain('Confirmation link sent to wade@example.com');
    });

    // AUTH-010: a new password, after a confirmation; then the menu says it was asked for.
    await screen.getByRole('button', { name: 'Actions', exact: true }).click();
    await expectAccessible(page.getByRole('menu', { name: 'Account actions' }).element());
    await page.getByRole('menuitem', { name: 'Ask for a new password' }).click();
    const confirm = page.getByRole('dialog', { name: 'Ask Wade Edited for a new password?' });
    await confirm.getByRole('button', { name: 'Ask for a new password' }).click();
    await expect.element(confirm).not.toBeInTheDocument();
    await screen.getByRole('button', { name: 'Actions', exact: true }).click();
    await expect
      .element(page.getByRole('menuitem', { name: 'New password asked for' }))
      .toHaveAttribute('aria-disabled', 'true');
    expect(calls.some((call) => call.path === '/users/801/password-change')).toBe(true);
    expect(await tooSmall(screen.container)).toEqual([]);
  });

  it('ACC-026: adds a person to a group and removes them from one', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    let current = profile(
      person(901, 'Gina Groups', {
        groups: [{ id: 3, name: 'Support', builtin: null, team_of: null }],
      }),
      'full',
    );
    const calls = fakeApi({
      'GET /users/901': () => ({ body: current }),
      'GET /users/901/roles': { body: { items: [] } },
      'GET /groups': {
        body: { items: [group(3, 'Support'), group(4, 'Sales')], next_cursor: null },
      },
      'POST /groups/4/members': () => {
        current = {
          ...current,
          groups: [...current.groups, { id: 4, name: 'Sales', builtin: null, team_of: null }],
          version: ++version,
        };
        return { body: { users: [901] } };
      },
      'POST /groups/3/members/remove': () => {
        current = {
          ...current,
          groups: current.groups.filter((item) => item.id !== 3),
          version: ++version,
        };
        return { body: { users: [901] } };
      },
    });
    const screen = await render(UserProfilePage, {
      profile: await profileOf(901),
      emailEnabled: true,
    });
    const groups = screen.getByRole('region', { name: 'Groups' });

    await groups.getByRole('button', { name: 'Add to group' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add Gina Groups to a group' });
    // The group she is in already is not offered.
    await expect.element(dialog.getByRole('radio', { name: /Sales/ })).toBeVisible();
    expect(dialog.getByRole('radio', { name: /Support/ }).elements()).toHaveLength(0);
    await dialog.getByRole('button', { name: 'Add to group' }).click();
    await expect.element(dialog.getByText('Choose a group.')).toBeVisible();
    await dialog.getByRole('radio', { name: /Sales/ }).click();
    await dialog.getByRole('button', { name: 'Add to group' }).click();
    await expect.element(groups.getByRole('link', { name: 'Sales' })).toBeVisible();

    await groups.getByRole('button', { name: 'Remove from “Support”' }).click();
    const confirm = page.getByRole('dialog', { name: 'Remove Gina Groups from “Support”?' });
    await confirm.getByRole('button', { name: 'Remove from group' }).click();
    await expect.element(groups.getByRole('link', { name: 'Support' })).not.toBeInTheDocument();
    expect(calls.some((call) => call.path === '/groups/3/members/remove')).toBe(true);
    expect(toastTexts()).toContain('Added to “Sales”: 1');
    await expectAccessible(screen.container);
  });

  it('ACC-059, ACC-061: roles show where they come from; one is assigned, a direct one revoked', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    let roles: EffectiveRole[] = [
      {
        role: { id: 3, name: 'Project member', builtin: 'project_member' },
        project_id: null,
        permissions: ['issue.read', 'issue.create'],
        sources: [
          { kind: 'direct', assignment_id: 31 },
          { kind: 'group', group: { id: 8, name: 'Developers', builtin: null, team_of: null } },
        ],
      },
      {
        role: { id: 4, name: 'Directory viewer', builtin: 'directory_viewer' },
        project_id: null,
        permissions: ['user.directory', 'user.self'],
        sources: [
          {
            kind: 'group',
            group: { id: 2, name: 'All members', builtin: 'all_members', team_of: null },
          },
        ],
      },
    ];
    const calls = fakeApi({
      'GET /permissions': { body: { permissions: CATALOG } },
      'GET /users/1001': { body: profile(person(1001, 'Wendy Roles'), 'full') },
      'GET /users/1001/roles': () => ({ body: { items: roles } }),
      'GET /roles': {
        body: {
          items: [
            role(3, 'Project member', 'project_member'),
            role(4, 'Directory viewer', 'directory_viewer'),
            role(9, 'Release managers'),
          ],
          next_cursor: null,
        },
      },
      'POST /role-assignments': () => {
        roles = [
          ...roles,
          {
            role: { id: 9, name: 'Release managers', builtin: null },
            project_id: null,
            permissions: ['issue.update'],
            sources: [{ kind: 'direct', assignment_id: 90 }],
          },
        ];
        return { status: 201, body: { items: [] } };
      },
      'DELETE /role-assignments/31': () => {
        roles = roles.map((held) =>
          held.role.id === 3
            ? { ...held, sources: held.sources.filter((source) => source.kind !== 'direct') }
            : held,
        );
        return { status: 204 };
      },
    });
    const screen = await render(UserProfilePage, {
      profile: await profileOf(1001),
      emailEnabled: true,
    });
    const section = screen.getByRole('region', { name: 'Roles' });

    await expect.element(section.getByRole('heading', { name: 'Project member' })).toBeVisible();
    await expect.element(section.getByText('Assigned directly')).toBeVisible();
    await expect
      .element(section.getByRole('link', { name: 'Developers' }))
      .toHaveAttribute('href', '/admin/groups/8');
    await expect.element(section.getByRole('link', { name: 'All members' })).toBeVisible();
    await expect.element(section.getByText('Global').first()).toBeVisible();
    await section.getByText('2 permissions').first().click();
    await expect.element(section.getByText('Read issues')).toBeVisible();
    await expectAccessible(screen.container);

    await section.getByRole('button', { name: 'Assign role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Assign a role to Wendy Roles' });
    const select = dialog.getByLabelText(label('Role'));
    await expect.element(select).toBeVisible();
    // A role held directly everywhere is not offered again.
    expect(dialog.getByRole('option', { name: 'Project member' }).elements()).toHaveLength(0);
    // Global is the first choice of people who change system settings.
    await expect.element(dialog.getByRole('radio', { name: 'Globally' })).toBeChecked();
    await expect.element(dialog.getByRole('radio', { name: 'In chosen projects' })).toBeVisible();
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog.getByText('Choose a role.')).toBeVisible();
    await select.selectOptions('9');
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(section.getByRole('heading', { name: 'Release managers' })).toBeVisible();
    expect(calls.find((call) => call.path === '/role-assignments')?.body).toEqual({
      role_id: 9,
      user_id: 1001,
      global: true,
      projects: [],
    });

    await section.getByRole('button', { name: 'Revoke “Project member”' }).click();
    const confirm = page.getByRole('dialog', { name: 'Revoke “Project member” from Wendy Roles?' });
    await confirm.getByRole('button', { name: 'Revoke' }).click();
    await expect
      .element(section.getByRole('button', { name: 'Revoke “Project member”' }))
      .not.toBeInTheDocument();
    expect(calls.some((call) => call.method === 'DELETE')).toBe(true);
    expect(toastTexts()).toEqual(expect.arrayContaining(['Role assigned', 'Role revoked']));
    expect(await tooSmall(screen.container)).toEqual([]);
  });

  it('ACC-059, ACC-061, ACC-033: assigns a role in chosen projects; roles through a team say so', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    const team = { id: 50, name: 'Web', builtin: null, team_of: 5 };
    let roles: EffectiveRole[] = [
      {
        role: { id: 3, name: 'Project member', builtin: 'project_member' },
        project_id: 5,
        permissions: ['issue.read'],
        sources: [{ kind: 'group', group: team }],
      },
    ];
    const calls = fakeApi({
      'GET /permissions': { body: { permissions: CATALOG } },
      'GET /users/1002': {
        body: profile(person(1002, 'Xenia Team', { groups: [team], projects: [WEB] }), 'full'),
      },
      'GET /users/1002/roles': () => ({ body: { items: roles } }),
      'GET /projects/5': { body: project(WEB) },
      'GET /projects/6': { body: project(MOBILE) },
      'GET /projects': { body: { items: [project(WEB), project(MOBILE)], next_cursor: null } },
      'GET /roles': {
        body: {
          items: [
            role(3, 'Project member', 'project_member'),
            { ...role(4, 'Directory viewer', 'directory_viewer'), permissions: ['user.directory'] },
            role(9, 'Release managers'),
          ],
          next_cursor: null,
        },
      },
      'POST /role-assignments': () => {
        roles = [
          ...roles,
          {
            role: { id: 9, name: 'Release managers', builtin: null },
            project_id: 6,
            permissions: ['issue.read'],
            sources: [{ kind: 'direct', assignment_id: 91 }],
          },
        ];
        return { status: 201, body: { items: [] } };
      },
    });
    const screen = await render(UserProfilePage, {
      profile: await profileOf(1002),
      emailEnabled: true,
    });

    // A team is shown with the projects, not with the groups.
    const groups = screen.getByRole('region', { name: 'Groups' });
    await expect.element(groups.getByText('Everyone')).toBeVisible();
    expect(groups.getByText('Web team').query()).toBeNull();
    const projects = screen.getByRole('region', { name: 'Projects' });
    await expect
      .element(projects.getByRole('link', { name: 'Web' }))
      .toHaveAttribute('href', '/projects/WEB');

    const section = screen.getByRole('region', { name: 'Roles' });
    await expect.element(section.getByText(/Through the team of/)).toBeVisible();
    // The scope and the team's project are both named, each a link to the project.
    await vi.waitFor(() => {
      expect(
        section
          .getByRole('link', { name: 'Web' })
          .elements()
          .map((link) => link.getAttribute('href')),
      ).toEqual(['/projects/WEB', '/projects/WEB']);
    });
    // A team role is not revoked here: it ends by leaving the team.
    expect(section.getByRole('button', { name: /^Revoke/ }).query()).toBeNull();

    await section.getByRole('button', { name: 'Assign role' }).click();
    const dialog = page.getByRole('dialog', { name: 'Assign a role to Xenia Team' });
    await dialog.getByRole('radio', { name: 'In chosen projects' }).click();
    // In projects only roles that give something there are offered.
    expect(dialog.getByRole('option', { name: 'Directory viewer' }).query()).toBeNull();
    await dialog.getByLabelText(label('Role')).selectOptions('9');
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog.getByText('Choose at least one project.')).toBeVisible();
    await dialog.getByRole('checkbox', { name: /^Mobile/ }).click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Assign role' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.find((call) => call.path === '/role-assignments')?.body).toEqual({
      role_id: 9,
      user_id: 1002,
      global: false,
      projects: [6],
    });
    await expect.element(section.getByRole('link', { name: 'Mobile' })).toBeVisible();
    await expect
      .element(section.getByRole('button', { name: 'Revoke “Release managers”' }))
      .toBeVisible();
    expect(await tooSmall(screen.container)).toEqual([]);
  });

  it('ACC-006: deleting from the profile says where links lead now', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(ADMIN);
    fakeApi({
      'GET /users/1101': { body: profile(person(1101, 'Lena Last'), 'full') },
      'GET /users/1101/roles': { body: { items: [] } },
      'GET /users': { body: { items: [person(1102, 'Mark Next')], next_cursor: null } },
      'POST /users/deletion-impact': {
        body: { accounts: 1, sessions: 0, tokens: 0, memberships: 0, assignments: 0 },
      },
      'POST /users/delete': { status: 204 },
    });
    const screen = await render(UserProfilePage, {
      profile: await profileOf(1101),
      emailEnabled: true,
    });
    await screen.getByRole('button', { name: 'Actions', exact: true }).click();
    await page.getByRole('menuitem', { name: 'Delete account' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete the account of Lena Last?' });
    await expect.element(dialog.getByText('Nothing else', { exact: false })).toBeVisible();
    await dialog.getByRole('radio', { name: /Mark Next/ }).click();
    await dialog.getByRole('button', { name: 'Delete account' }).click();
    await expect.element(screen.getByRole('heading', { name: 'Account deleted' })).toBeVisible();
    await expect.element(screen.getByText('Links to it now lead to Mark Next.')).toBeVisible();
  });
});
