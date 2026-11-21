// The «Возможности» screen on a desktop and on a phone (ACC-064, ADM-023; plan step 1.11 §6.1,
// §7.3): the group multiselect with its search, a save per row, the confirmation when a save
// switches a capability off for everybody, and the «Общие для всех» flags — empty in P1, so the
// section renders only when the registry has any.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  AccessMap,
  AdminFlag,
  AdminSettings,
  Capability,
  Capabilities,
  Group,
  Session,
} from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi } from '../../../../test/fake-api.ts';
import CapabilitiesPage from './CapabilitiesPage.svelte';

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
  created_at: '2026-11-08T09:00:00Z',
  password_change_required: false,
};

const ADMIN: AccessMap = {
  global: ['system.read', 'system.write'],
  projects: [],
  capabilities: [],
};
const READER: AccessMap = { global: ['system.read'], projects: [], capabilities: [] };

function group(id: number, name: string, extra: Partial<Group> = {}): Group {
  return {
    id,
    name,
    builtin: null,
    team_of: null,
    auto_join: false,
    created_at: '2026-01-01T00:00:00Z',
    description: '',
    members: 2,
    roles: [],
    version: 1,
    ...extra,
  };
}

const GROUPS: Group[] = [
  group(1, 'Everyone', { builtin: 'everyone' }),
  group(2, 'Designers'),
  group(3, 'Support'),
];

/** `share_views` on for one group, as the screen loads it. */
function capability(groups: number[]): Capability {
  return {
    key: 'share_views',
    can_be_switched_off: true,
    groups: groups.map((id) => {
      const found = GROUPS.find((group) => group.id === id);
      return {
        id,
        name: found?.name ?? '',
        builtin: found?.builtin ?? null,
        team_of: null,
      };
    }),
  };
}

function flag(key: string, extra: Partial<AdminFlag> = {}): AdminFlag {
  return {
    key,
    description: 'Everything about tags says who added them.',
    value: true,
    default: true,
    is_stored: false,
    may_switch_off: true,
    ...extra,
  };
}

/** The settings answer of the capabilities screen; only the flags matter here. */
function settings(flags: AdminFlag[]): AdminSettings {
  return { settings: [], flags, upload_ceiling: 0 };
}

/** The fake server; `stored` is what `/capabilities` answers before any change. */
function server(flags: AdminFlag[], stored: Capabilities = { items: [capability([1])] }) {
  setSessionForTests(SESSION);
  setAccessForTests(ADMIN);
  let current = stored;
  return fakeApi({
    'GET /capabilities': () => ({ body: current }),
    'PUT /capabilities/share_views': (request) => {
      const body = request.body as { groups: number[] };
      current = { items: [capability(body.groups)] };
      return { body: current.items[0] };
    },
    'GET /groups': () => ({ body: { items: GROUPS, next_cursor: null } }),
    'GET /admin/settings': () => ({ body: settings(flags) }),
    'PUT /admin/settings': (request) => {
      const body = request.body as Record<string, boolean | null>;
      const changed = flags.map((one) =>
        body[`flag.${one.key}`] === undefined
          ? one
          : {
              ...one,
              value: body[`flag.${one.key}`] ?? one.default,
              is_stored: body[`flag.${one.key}`] !== null,
            },
      );
      return { body: settings(changed) };
    },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('CapabilitiesPage', () => {
  it('ACC-064: a row per capability with its groups; a change saves; the search narrows the multiselect', async () => {
    const calls = server([]);

    const screen = await render(CapabilitiesPage, { capabilities: null });

    await expect.element(screen.getByRole('heading', { name: 'Sharing settings' })).toBeVisible();
    await expect.element(screen.getByRole('checkbox', { name: 'Everyone' })).toBeChecked();
    await expect.element(screen.getByRole('checkbox', { name: 'Designers' })).not.toBeChecked();
    // Nothing changed: the save waits.
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    // The search narrows the choices; a chosen group stays listed anyway.
    await userEvent.fill(screen.getByLabelText('Find groups'), 'des');
    await expect.element(screen.getByRole('checkbox', { name: 'Designers' })).toBeVisible();
    await expect.element(screen.getByRole('checkbox', { name: 'Support' })).not.toBeInTheDocument();
    await expect.element(screen.getByRole('checkbox', { name: 'Everyone' })).toBeVisible();

    await screen.getByRole('checkbox', { name: 'Designers' }).click();
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
    await screen.getByRole('button', { name: 'Save' }).click();

    const put = calls.find(
      (call) => call.method === 'PUT' && call.path === '/capabilities/share_views',
    );
    expect(put?.body).toEqual({ groups: [1, 2] });
    await expect.element(screen.getByRole('checkbox', { name: 'Designers' })).toBeChecked();
    expect(toasts.items.at(-1)?.message).toBe('Capability saved');
    if (isPhone()) expect(await smallTargets(screen.container)).toEqual([]);
  });

  it('ADM-023: emptying the list of a switchable capability asks before switching it off for everybody', async () => {
    const calls = server([]);

    const screen = await render(CapabilitiesPage, { capabilities: null });
    await screen.getByRole('checkbox', { name: 'Everyone' }).click();
    await expect
      .element(screen.getByText('Off: the list is empty, so nobody has it.'))
      .toBeVisible();
    await screen.getByRole('button', { name: 'Save' }).click();

    // The confirmation asks what an empty list means; saying no changes nothing.
    const dialog = page.getByRole('dialog', { name: 'Switch off for everybody?' });
    await expect
      .element(dialog.getByText('The list is empty, so nobody will have this capability.'))
      .toBeVisible();
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    expect(
      calls.filter((call) => call.method === 'PUT' && call.path === '/capabilities/share_views'),
    ).toHaveLength(0);

    await screen.getByRole('button', { name: 'Save' }).click();
    await dialog.getByRole('button', { name: 'Switch off' }).click();
    const put = calls.find(
      (call) => call.method === 'PUT' && call.path === '/capabilities/share_views',
    );
    expect(put?.body).toEqual({ groups: [] });
    expect(toasts.items.at(-1)?.message).toBe('Capability saved');
  });

  it('ADM-023: the «Общие для всех» flags stay away while the registry is empty', async () => {
    server([]);

    const screen = await render(CapabilitiesPage, { capabilities: null });

    await expect.element(screen.getByRole('heading', { name: 'Sharing settings' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'On for the whole instance' }).query()).toBeNull();
  });

  it('ADM-023: a flag of the registry switches and returns to its default; one that may not go off stays put', async () => {
    const calls = server([
      flag('tag_history', { description: 'Everything about tags says who added them.' }),
      flag('license', { description: 'The license banner shows.', may_switch_off: false }),
    ]);

    const screen = await render(CapabilitiesPage, { capabilities: null });

    await expect
      .element(screen.getByRole('heading', { name: 'On for the whole instance' }))
      .toBeVisible();
    const history = screen.getByRole('checkbox', {
      name: 'Everything about tags says who added them.',
    });
    await expect.element(history).toBeChecked();
    await expect.element(screen.getByText('Differs from the default.')).not.toBeInTheDocument();

    await history.click();
    const put = calls.find((call) => call.method === 'PUT' && call.path === '/admin/settings');
    expect(put?.body).toEqual({ 'flag.tag_history': false });
    await expect.element(screen.getByText('Differs from the default.')).toBeVisible();
    expect(toasts.items.at(-1)?.message).toBe('Switch saved');

    // Back to the default: `null` clears the stored row.
    await screen.getByRole('button', { name: 'Return to the default' }).click();
    const reset = calls
      .filter((call) => call.method === 'PUT' && call.path === '/admin/settings')
      .at(-1);
    expect(reset?.body).toEqual({ 'flag.tag_history': null });

    // A flag that may not be switched off is not changeable at all.
    await expect
      .element(screen.getByRole('checkbox', { name: 'The license banner shows.' }))
      .toBeDisabled();
  });

  it('ACC-035: a reader without system.write sees the note and no working controls', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(READER);
    fakeApi({
      'GET /capabilities': () => ({ body: { items: [capability([1])] } }),
      'GET /groups': () => ({ body: { items: GROUPS, next_cursor: null } }),
      'GET /admin/settings': () => ({ body: settings([]) }),
    });

    const screen = await render(CapabilitiesPage, { capabilities: null });

    await expect
      .element(screen.getByText('Only system administrators can change capabilities.'))
      .toBeVisible();
    await expect.element(screen.getByRole('checkbox', { name: 'Everyone' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Save' }).query()).toBeNull();
  });

  it('ADM-023: the screen is accessible on both profiles; Russian texts', async () => {
    setLocale('ru');
    server([]);

    const screen = await render(CapabilitiesPage, { capabilities: null });

    await expect.element(screen.getByRole('heading', { name: 'Настройка доступа' })).toBeVisible();
    await expect
      .element(
        screen.getByText(
          'Кто может менять, кто смотрит, использует и редактирует теги, сохранённые поиски и доски. По умолчанию — все.',
        ),
      )
      .toBeVisible();
    await expect.element(screen.getByRole('checkbox', { name: 'Все' })).toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });
});
