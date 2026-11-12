// The «Настройки» screen of the administration area on a desktop and on a phone (plan step 1.11
// §4, §7.2; ADM-002, ADM-003, ADM-004, ADM-015, ADM-016, ADM-017, AUTH-004, UI-004): three tabs,
// a save per tab, the address warnings and the checklist of a change, the upload ceiling and the
// session bounds, and the instance's home page marked in the account chooser.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import { setBootstrapForTests } from '#lib/api/system.ts';
import type { AccessMap, AdminSettings, Bootstrap, Session } from '#lib/api/schema.gen.ts';
import InterfaceSettings from '#lib/features/account/InterfaceSettings.svelte';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import {
  preferences,
  setAccessForTests,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest } from '../../../../test/fake-api.ts';
import SettingsPage from './SettingsPage.svelte';

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

const MIB = 1024 * 1024;
const GIB = 1024 * 1024 * 1024;

/** The registry's non-secret, non-mail settings, in the registry's order (part 1.11-1). */
function settings(overrides: Record<string, unknown> = {}): AdminSettings {
  const values: Record<string, unknown> = {
    base_url: null,
    system_language: 'en',
    default_time_zone: 'UTC',
    default_home: 'issues',
    default_date_format: 'locale',
    'api.allowed_origins': [],
    upload_max_bytes: 10 * MIB,
    session_idle_minutes: 60,
    remember_me_days: 30,
    ...overrides,
  };
  const defaults: Record<string, unknown> = {
    base_url: null,
    system_language: 'en',
    default_time_zone: 'UTC',
    default_home: 'issues',
    default_date_format: 'locale',
    'api.allowed_origins': [],
    upload_max_bytes: 10 * MIB,
    session_idle_minutes: 60,
    remember_me_days: 30,
  };
  return {
    settings: Object.keys(values).map((key) => ({
      key,
      value: values[key],
      default: defaults[key],
      is_stored: values[key] !== defaults[key],
    })),
    flags: [],
    upload_ceiling: 2 * GIB,
  };
}

const BOOTSTRAP: Bootstrap = {
  base_url: null,
  default_date_format: 'locale',
  default_home: 'projects',
  email_enabled: false,
  gravatar: false,
  limits: { upload_max_bytes: 10 * MIB },
  setup_required: false,
  version: '1.11.0',
  work_schedule: { day_minutes: 480, week_days: 5 },
};

/** The fake server with the settings routes; `stored` answers both reads and writes. */
function server(access: AccessMap, stored: AdminSettings): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi({
    'GET /admin/settings': { body: stored },
    'PUT /admin/settings': (request) => {
      const change: Record<string, unknown> = {
        ...Object.fromEntries(stored.settings.map((one) => [one.key, one.value])),
        ...(request.body as Record<string, unknown>),
      };
      return { body: settings(change) };
    },
  });
}

/** A field by its label; required fields add "*" to the label text. */
function label(text: string): RegExp {
  return new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\*?$`);
}

function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  setPreferencesForTests(null);
  setBootstrapForTests(null);
  setLocale(BASE_LOCALE);
});

describe('SettingsPage', () => {
  it('ADM-002: three tabs render; “О системе” says the version from the bootstrap', async () => {
    setBootstrapForTests(BOOTSTRAP);
    server(ADMIN, settings());

    const screen = await render(SettingsPage, { settings: null });

    const tabs = screen.getByRole('tablist', { name: 'Settings sections' });
    await expect.element(tabs.getByRole('tab', { name: 'Server' })).toBeVisible();
    await expect.element(tabs.getByRole('tab', { name: 'Localization' })).toBeVisible();
    await expect.element(tabs.getByRole('tab', { name: 'About' })).toBeVisible();
    // The server tab is the one shown first; the version waits for its own tab.
    await expect
      .element(screen.getByRole('spinbutton', { name: label('Session ends after') }))
      .toBeVisible();
    expect(screen.getByText('1.11.0').query()).toBeNull();

    await screen.getByRole('tab', { name: 'About' }).click();
    await expect.element(screen.getByText('1.11.0')).toBeVisible();
    expect(screen.container.textContent).toContain('Version');
  });

  it('ADM-004, AUTH-004, UI-004: the server tab saves every key of its own; nothing else rides along', async () => {
    const calls = server(ADMIN, settings());

    const screen = await render(SettingsPage, { settings: null });
    await screen.getByLabelText(label('Instance address')).fill('https://track.example.com');
    await screen.getByLabelText(label('Largest attachment')).fill('20');
    await screen.getByLabelText(label('Session ends after')).fill('120');
    await screen.getByLabelText(label('“Remember me” lasts')).fill('14');
    await screen.getByRole('radio', { name: 'Boards' }).click();
    await screen.getByRole('button', { name: 'Save' }).click();

    const put = calls.find((call) => call.method === 'PUT' && call.path === '/admin/settings');
    expect(put?.body).toEqual({
      base_url: 'https://track.example.com',
      'api.allowed_origins': [],
      upload_max_bytes: 20 * MIB,
      session_idle_minutes: 120,
      remember_me_days: 14,
      default_home: 'boards',
    });
    // The answer's values stand in the fields (a number input reads back as a number).
    await expect
      .element(screen.getByRole('spinbutton', { name: label('Session ends after') }))
      .toHaveValue(120);
  });

  it('INT-056: the origins editor saves the lines, refuses a path, and empty means same-origin only', async () => {
    const calls = server(ADMIN, settings({ 'api.allowed_origins': ['https://app.example.com'] }));

    const screen = await render(SettingsPage, { settings: null });
    // The stored origins sit in the field, one per line.
    await expect
      .element(screen.getByLabelText(label('API origins')))
      .toHaveValue('https://app.example.com');

    // Typing two more lines saves exactly the typed origins; blank lines drop away.
    const field = screen.getByLabelText(label('API origins'));
    await field.fill('https://app.example.com\nhttp://localhost:5173\n\n');
    await screen.getByRole('button', { name: 'Save' }).click();
    const put = calls.find((call) => call.method === 'PUT' && call.path === '/admin/settings');
    expect((put?.body as Record<string, unknown>)['api.allowed_origins']).toEqual([
      'https://app.example.com',
      'http://localhost:5173',
    ]);

    // A line with a path is refused in the field, and the save cannot leave with it: the button
    // itself refuses while an origin is invalid, and no second PUT happens.
    await field.fill('https://app.example.com/app');
    await expect
      .element(
        screen.getByText(
          'Each line is an origin: https://app.example.com, at most a port, no path.',
        ),
      )
      .toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(calls.filter((call) => call.method === 'PUT')).toHaveLength(1);
  });

  it('ADM-004: megabyte and gigabyte units; the deployment ceiling is shown and enforced', async () => {
    const calls = server(ADMIN, settings());

    const screen = await render(SettingsPage, { settings: null });

    // 2 GB ceiling in the field's own hint.
    await expect.element(screen.getByText('This server allows at most 2 GB.')).toBeVisible();

    await screen.getByLabelText(label('Largest attachment')).fill('5');
    await screen.getByLabelText(label('Unit')).selectOptions('gb');
    await expect.element(screen.getByText('More than the server allows (2 GB).')).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(calls.filter((call) => call.method === 'PUT')).toHaveLength(0);

    await screen.getByLabelText(label('Largest attachment')).fill('2');
    await screen.getByLabelText(label('Unit')).selectOptions('gb');
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
    await screen.getByRole('button', { name: 'Save' }).click();
    const put = calls.find((call) => call.method === 'PUT' && call.path === '/admin/settings');
    expect(put?.body).toMatchObject({ upload_max_bytes: 2 * GIB });
  });

  it('AUTH-004: the session bounds are in the hints and in the fields', async () => {
    server(ADMIN, settings());

    const screen = await render(SettingsPage, { settings: null });

    await screen.getByLabelText(label('Session ends after')).fill('5');
    await expect.element(screen.getByText('From 10 to 720.')).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    await screen.getByLabelText(label('Session ends after')).fill('720');
    await screen.getByLabelText(label('“Remember me” lasts')).fill('400');
    await expect.element(screen.getByText('From 1 to 365.')).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    await screen.getByLabelText(label('“Remember me” lasts')).fill('365');
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
  });

  it('ADM-003: an http address warns; an address that differs from this page warns; a change shows the checklist', async () => {
    server(ADMIN, settings({ base_url: 'https://elsewhere.example.com' }));

    const screen = await render(SettingsPage, { settings: null });

    // The stored address differs from where this page came from.
    await expect
      .element(
        screen.getByText(
          new RegExp(
            `^This differs from the address this page came from \\(${escape(location.origin)}\\)`,
          ),
        ),
      )
      .toBeVisible();
    // The stored address is fine as to scheme: no http warning yet.
    expect(screen.getByText(/starts with http/).query()).toBeNull();

    await screen.getByLabelText(label('Instance address')).fill('http://plain.example.com');
    await expect.element(screen.getByText(/starts with http:/)).toBeVisible();

    await screen.getByLabelText(label('Instance address')).fill('https://track.example.com');
    await screen.getByRole('button', { name: 'Save' }).click();
    await expect
      .element(screen.getByText('letters and invitations now link to the new address;'))
      .toBeVisible();
    await expect
      .element(screen.getByText('the old address keeps working while traffic still reaches it.'))
      .toBeVisible();

    // Editing the field again puts the checklist away.
    await screen.getByLabelText(label('Instance address')).fill('https://other.example.com');
    expect(screen.getByText(/link to the new address/).query()).toBeNull();
  });

  it('ADM-003: an address that is not a full http(s) URL is refused in the field', async () => {
    const calls = server(ADMIN, settings());

    const screen = await render(SettingsPage, { settings: null });
    await screen.getByLabelText(label('Instance address')).fill('track.example.com');
    await expect
      .element(screen.getByText('A full address starting with http:// or https://.'))
      .toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(calls.filter((call) => call.method === 'PUT')).toHaveLength(0);
  });

  it('ADM-015, ADM-016, ADM-017: the localization tab saves its three keys; zones search; formats preview', async () => {
    const calls = server(ADMIN, settings());

    const screen = await render(SettingsPage, { settings: null });
    await screen.getByRole('tab', { name: 'Localization' }).click();

    await screen.getByLabelText(label('System language')).selectOptions('ru');
    await screen.getByLabelText(label('Find a zone')).fill('Moscow');
    const zoneSelect = screen.getByLabelText(label('Time zone')).element() as HTMLSelectElement;
    const found = Array.from(zoneSelect.options).map((option) => option.value);
    expect(found).toContain('Europe/Moscow');
    expect(found).not.toContain('America/New_York');
    await screen.getByLabelText(label('Time zone')).selectOptions('Europe/Moscow');

    // The four formats of §1, previewed on the sample.
    await expect.element(screen.getByText('31.12.2000, 23:59')).toBeVisible();
    await expect.element(screen.getByText('2000-12-31 23:59')).toBeVisible();
    await expect.element(screen.getByText('31 Dec 2000, 23:59')).toBeVisible();
    await screen.getByRole('radio', { name: 'Year first (ISO)' }).click();
    await screen.getByRole('button', { name: 'Save' }).click();

    const put = calls.find((call) => call.method === 'PUT' && call.path === '/admin/settings');
    expect(put?.body).toEqual({
      system_language: 'ru',
      default_time_zone: 'Europe/Moscow',
      default_date_format: 'iso',
    });
  });

  it('ADM-002: a reader without system.write sees the notice and no working fields', async () => {
    server(READER, settings());

    const screen = await render(SettingsPage, { settings: null });

    await expect
      .element(screen.getByText('Only system administrators can change these settings.'))
      .toBeVisible();
    await expect
      .element(screen.getByRole('spinbutton', { name: label('Session ends after') }))
      .toBeDisabled();
    expect(screen.getByRole('button', { name: 'Save' }).query()).toBeNull();
  });

  it('ADM-002, ADM-017: the screen is accessible on both profiles', async () => {
    server(ADMIN, settings());

    const screen = await render(SettingsPage, { settings: null });
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await screen.getByRole('tab', { name: 'Localization' }).click();
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ADM-002: Russian texts', async () => {
    setLocale('ru');
    server(ADMIN, settings());

    const screen = await render(SettingsPage, { settings: null });

    await expect.element(screen.getByRole('tab', { name: 'Сервер' })).toBeVisible();
    await expect.element(screen.getByRole('tab', { name: 'Локализация' })).toBeVisible();
    await expect.element(screen.getByRole('tab', { name: 'О системе' })).toBeVisible();
    await expect.element(screen.getByText('Главная страница')).toBeVisible();

    await screen.getByRole('tab', { name: 'Локализация' }).click();
    await expect.element(screen.getByLabelText(label('Часовой пояс'))).toBeVisible();
    await expect.element(screen.getByText('31 дек. 2000, 23:59')).toBeVisible();
  });
});

describe('the instance home page (UI-004)', () => {
  it('falls back to the instance default when the person has not chosen', () => {
    setBootstrapForTests(BOOTSTRAP); // default_home: 'projects'
    setPreferencesForTests({});
    expect(preferences.value('home')).toBe('projects');

    preferences.set({ home: 'boards' });
    expect(preferences.value('home')).toBe('boards');

    preferences.set({ home: null });
    expect(preferences.value('home')).toBe('projects');
  });

  it('is marked in the «Интерфейс» chooser of the account page', async () => {
    setBootstrapForTests(BOOTSTRAP);
    setPreferencesForTests({});

    const screen = await render(InterfaceSettings);

    const projects = screen.getByRole('radio', { name: 'Projects' });
    await expect.element(projects).toBeVisible();
    // The mark is the hint beside the option, not part of its name.
    await expect.element(screen.getByText('The instance default')).toBeVisible();
    expect(screen.container.querySelectorAll('.hint')).toHaveLength(1);
  });
});
