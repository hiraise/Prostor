// A project's general settings on a desktop and on a phone (step 1.2; PRJ-002, PRJ-003, PRJ-005,
// PRJ-006, PRJ-008, PRJ-037…PRJ-039, PRJ-061, PRJ-063, PRJ-064).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Group, Project, Session, UserRef } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { openProject, projectRow } from '../projects.ts';
import SettingsNav from '../SettingsNav.svelte';
import GeneralSettings from './GeneralSettings.svelte';

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

const BORIS: UserRef = {
  id: 2,
  full_name: 'Boris Lead',
  username: 'boris',
  avatar_color: 2,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

/** Administers project 1 and may delete it. */
const OWNER: AccessMap = {
  global: [],
  projects: [
    {
      project_id: 1,
      permissions: ['project.basic', 'project.full', 'project.update', 'project.delete'],
    },
  ],
  capabilities: [],
};

/** Configures project 1 but may not delete it. */
const MANAGER: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic', 'project.full', 'project.update'] }],
  capabilities: [],
};

const WEB: Project = {
  id: 1,
  name: 'Web',
  key: 'WEB',
  archived_at: null,
  created_at: '2026-03-14T09:00:00Z',
  default_audience: null,
  deletion: null,
  description: 'The public site.',
  description_html: '<p>The public site.</p>\n',
  favorite: false,
  former_keys: ['SITE'],
  kind: 'standard',
  logo_color: 2,
  logo_url: null,
  next_number: 1,
  owner: BORIS,
  team: { id: 101, name: 'Web', builtin: null, team_of: 1 },
  team_size: 4,
  template: 'blank',
  version: 3,
};

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

function server(access: AccessMap, routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi({
    'GET /users/me/access': { body: access },
    'GET /projects/check': { body: { fields: [], suggested_key: null } },
    ...routes,
  });
}

/** A field by its label; required fields add "*" to the label text. */
function label(text: string): RegExp {
  return new RegExp(`^${text}\\s*\\*?$`);
}

/** Loads the project the way the route does and renders its settings. */
async function settingsPage(
  project: Project,
  props: { ondeleted?: () => unknown; onkeychanged?: () => unknown } = {},
): Promise<ReturnType<typeof render>> {
  const opened = await openProject(project.key);
  if (opened.kind !== 'found') throw new Error(`project ${project.key} is ${opened.kind}`);
  const row = projectRow(project.id);
  if (!row) throw new Error('no row');
  return render(GeneralSettings, {
    project: row,
    ondeleted: props.ondeleted ?? vi.fn(),
    onkeychanged: props.onkeychanged,
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
  store.entities.table('project').clear();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
});

describe('GeneralSettings', () => {
  it('PRJ-038, PRJ-002, PRJ-003, PRJ-006, PRJ-008: saves the name, key and description on the version shown', async () => {
    let current = WEB;
    const calls = server(OWNER, {
      'GET /projects/by-key/WEB': () => ({ body: current }),
      'PATCH /projects/1': ({ body }) => {
        const patch = body as { name: string; key: string; description: string };
        current = {
          ...current,
          ...patch,
          former_keys: [...current.former_keys, current.key],
          version: 4,
        };
        return { body: current };
      },
    });
    const onkeychanged = vi.fn();
    const screen = await settingsPage(WEB, { onkeychanged });

    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'General settings' }))
      .toBeVisible();
    await expect.element(screen.getByText('Boris Lead')).toBeVisible();
    expect(screen.container.querySelector('time')?.getAttribute('datetime')).toContain(
      '2026-03-14',
    );
    await expect.element(screen.getByRole('button', { name: 'Save changes' })).toBeDisabled();
    await screen.getByLabelText(label('Name')).fill('Website');
    await screen.getByLabelText(label('Key')).fill('site2');
    await screen.getByLabelText(label('Description')).fill('The public site.\nNew pages.');
    await vi.waitFor(() => {
      expect(
        calls.some((call) => call.path === '/projects/check' && call.query.get('key') === 'SITE2'),
      ).toBe(true);
    });
    await screen.getByRole('button', { name: 'Save changes' }).click();

    await vi.waitFor(() => {
      expect(onkeychanged).toHaveBeenCalledOnce();
    });
    expect(calls.find((call) => call.method === 'PATCH')?.body).toEqual({
      name: 'Website',
      key: 'SITE2',
      description: 'The public site.\nNew pages.',
      version: 3,
    });
    await expect.element(screen.getByLabelText(label('Name'))).toHaveValue('Website');
    await expect.element(screen.getByRole('button', { name: 'Save changes' })).toBeDisabled();
    expect(projectRow(1)?.former_keys).toEqual(['SITE', 'WEB']);
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('PRJ-006, CNT-029: the description is Markdown with formatting buttons and a preview', async () => {
    const calls = server(OWNER, {
      'GET /projects/by-key/WEB': { body: WEB },
      'POST /markdown': {
        body: {
          html: '<h1>Goals</h1>\n<p>The public site.</p>\n',
          refs: { users: [], issues: [] },
        },
      },
    });
    const screen = await settingsPage(WEB);

    const description = screen.getByLabelText(label('Description'));
    await expect
      .element(screen.getByRole('toolbar', { name: 'Formatting of “Description”' }))
      .toBeVisible();
    const area = description.element();
    if (!(area instanceof HTMLTextAreaElement)) throw new Error('no text field');
    area.focus();
    area.setSelectionRange(0, 0);
    await screen.getByRole('button', { name: 'Heading' }).click();
    await expect.element(description).toHaveValue('### The public site.');
    await screen.getByRole('tab', { name: 'Preview' }).click();
    await expect.element(screen.getByRole('heading', { level: 3, name: 'Goals' })).toBeVisible();
    expect(calls.find((call) => call.path === '/markdown')?.body).toEqual({
      text: '### The public site.',
      profile: 'basic',
      issue: null,
    });
    await expect.element(screen.getByRole('button', { name: 'Save changes' })).toBeEnabled();
    await expectAccessible(screen.container);
  });

  it('PRJ-005, PRJ-018: former keys are listed and stay the project’s own; a taken key is said while typing', async () => {
    const calls = server(OWNER, {
      'GET /projects/by-key/WEB': { body: WEB },
      'GET /projects/check': (request) => ({
        body: {
          fields:
            request.query.get('key') === 'HR' ? [{ path: 'key', code: 'taken', message: 'x' }] : [],
          suggested_key: null,
        },
      }),
    });
    const screen = await settingsPage(WEB);

    const former = screen.getByRole('group', { name: 'Former keys' });
    await expect.element(former.getByText('SITE', { exact: true })).toBeVisible();
    await expect.element(former.getByText('still lead here', { exact: false })).toBeVisible();
    await expect
      .element(screen.getByText('the old IDs keep leading to them', { exact: false }))
      .toBeVisible();

    await screen.getByLabelText(label('Key')).fill('site');
    // Long enough for the check to have run if it were going to.
    await new Promise((done) => setTimeout(done, 450));
    expect(calls.some((call) => call.path === '/projects/check')).toBe(false);

    await screen.getByLabelText(label('Key')).fill('hr');
    await expect.element(screen.getByText('Another project has or had this key.')).toBeVisible();
  });

  it('PRJ-038: a clash with someone else’s change keeps their values and says so', async () => {
    let current = WEB;
    server(OWNER, {
      'GET /projects/by-key/WEB': () => ({ body: current }),
      'GET /projects/1': () => ({ body: current }),
      'PATCH /projects/1': () => {
        current = { ...current, description: 'Their description', version: 5 };
        return failure(409, 'conflict');
      },
    });
    const screen = await settingsPage(WEB);

    await screen.getByLabelText(label('Description')).fill('Our description');
    await screen.getByRole('button', { name: 'Save changes' }).click();
    await expect
      .element(
        screen.getByText('Someone changed these settings at the same time', { exact: false }),
      )
      .toBeVisible();
    await expect
      .element(screen.getByLabelText(label('Description')))
      .toHaveValue('Their description');
    await expectAccessible(screen.container);
  });

  it('PRJ-039: the default audience is chosen among groups and teams', async () => {
    let current = WEB;
    const calls = server(OWNER, {
      'GET /projects/by-key/WEB': () => ({ body: current }),
      'GET /groups': {
        body: {
          items: [
            group(7, 'Developers', { description: 'People who write the code' }),
            group(101, 'Web', { team_of: 1 }),
          ],
          next_cursor: null,
        },
      },
      'PATCH /projects/1': ({ body }) => {
        const patch = body as { default_audience_id?: number | null };
        current = {
          ...current,
          default_audience:
            patch.default_audience_id === 101
              ? { id: 101, name: 'Web', builtin: null, team_of: 1 }
              : null,
          version: current.version + 1,
        };
        return { body: current };
      },
    });
    const screen = await settingsPage(WEB);

    const audience = screen.getByRole('group', { name: 'Default audience' });
    await expect.element(audience.getByText('Everyone who can see the project')).toBeVisible();
    await audience.getByRole('button', { name: 'Choose group or team' }).click();
    const dialog = page.getByRole('dialog', { name: 'Default audience' });
    await expect.element(dialog.getByRole('radio', { name: /^Developers/ })).toBeVisible();
    expect(calls.find((call) => call.path === '/groups')?.query.get('kind')).toBe('all');
    await expect
      .element(dialog.getByRole('radio', { name: 'Everyone who can see the project' }))
      .toBeChecked();
    await dialog.getByRole('radio', { name: /^Web team/ }).click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Choose', exact: true }).click();
    await expect.element(dialog).not.toBeInTheDocument();

    await expect.element(audience.getByText('Web team')).toBeVisible();
    await screen.getByRole('button', { name: 'Save changes' }).click();
    await vi.waitFor(() => {
      expect(calls.filter((call) => call.method === 'PATCH').at(-1)?.body).toEqual({
        default_audience_id: 101,
        version: 3,
      });
    });
    await expect.element(screen.getByRole('button', { name: 'Save changes' })).toBeDisabled();

    await audience.getByRole('button', { name: 'Remove restriction' }).click();
    await expect.element(audience.getByText('Everyone who can see the project')).toBeVisible();
    await screen.getByRole('button', { name: 'Save changes' }).click();
    await vi.waitFor(() => {
      expect(calls.filter((call) => call.method === 'PATCH').at(-1)?.body).toEqual({
        default_audience_id: null,
        version: 4,
      });
    });
  });

  it('PRJ-061, PRJ-063: archiving explains the effects and leaves only restore and delete', async () => {
    const calls = server(OWNER, {
      'GET /projects/by-key/WEB': { body: WEB },
      'POST /projects/1/archive': {
        body: { ...WEB, archived_at: '2026-10-22T10:00:00Z', version: 4 },
      },
      'POST /projects/1/restore': { body: { ...WEB, version: 5 } },
    });
    const screen = await settingsPage(WEB);

    await screen.getByRole('button', { name: 'Archive project' }).click();
    const dialog = page.getByRole('dialog', { name: 'Archive “Web”?' });
    await expect
      .element(dialog.getByText('leaves project pickers and searches', { exact: false }))
      .toBeVisible();
    await dialog.getByRole('button', { name: 'Archive project' }).click();
    await expect.element(dialog).not.toBeInTheDocument();

    // Read-only now: no saving, no audience choice; restore and delete stay.
    await expect.element(screen.getByLabelText(label('Name'))).toHaveAttribute('readonly');
    await expect.element(screen.getByLabelText(label('Description'))).toHaveAttribute('readonly');
    expect(screen.getByRole('button', { name: 'Save changes' }).query()).toBeNull();
    expect(screen.getByRole('button', { name: 'Choose group or team' }).query()).toBeNull();
    expect(screen.getByRole('button', { name: 'Archive project' }).query()).toBeNull();
    await expect.element(screen.getByRole('button', { name: 'Delete project' })).toBeVisible();
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await screen.getByRole('button', { name: 'Restore project' }).click();
    await expect.element(screen.getByRole('button', { name: 'Save changes' })).toBeVisible();
    await expect.element(screen.getByLabelText(label('Name'))).not.toHaveAttribute('readonly');
    expect(calls.filter((call) => call.method === 'POST').map((call) => call.path)).toEqual([
      '/projects/1/archive',
      '/projects/1/restore',
    ]);
  });

  it('PRJ-064: deleting says how many issues go and needs the project’s name typed exactly', async () => {
    const calls = server(OWNER, {
      'GET /projects/by-key/WEB': { body: WEB },
      'DELETE /projects/1': { status: 202 },
      'GET /issues/count': (request) =>
        request.query.get('project') === '1'
          ? { body: { all: 1234, unresolved: 56 } }
          : failure(404, 'not_found'),
    });
    const ondeleted = vi.fn();
    const screen = await settingsPage(WEB, { ondeleted });

    await screen.getByRole('button', { name: 'Delete project' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete “Web”?' });
    await expect.element(dialog.getByText('This cannot be undone', { exact: false })).toBeVisible();
    await expect
      .element(dialog.getByText(/^The project has 1,234 issues\. Every issue and article/))
      .toBeVisible();
    const confirm = dialog.getByLabelText('Type the project name to confirm');
    await expect.element(confirm).toHaveFocus();
    await confirm.fill('Web ');
    await dialog.getByRole('button', { name: 'Delete project' }).click();
    await expect.element(dialog.getByText('The name does not match.')).toBeVisible();
    expect(calls.some((call) => call.method === 'DELETE')).toBe(false);

    await confirm.fill('Web');
    await dialog.getByRole('button', { name: 'Delete project' }).click();
    await vi.waitFor(() => {
      expect(ondeleted).toHaveBeenCalledOnce();
    });
    expect(calls.some((call) => call.method === 'DELETE' && call.path === '/projects/1')).toBe(
      true,
    );
    expect(projectRow(1)).toBeUndefined();
  });

  it('PRJ-064: the issue count in Russian, and only the general warning when counting fails', async () => {
    setLocale('ru');
    let fails = false;
    server(OWNER, {
      'GET /projects/by-key/WEB': { body: WEB },
      'GET /issues/count': () =>
        fails ? failure(500, 'internal') : { body: { all: 3, unresolved: 1 } },
    });
    const screen = await settingsPage(WEB);
    await screen.getByRole('button', { name: 'Удалить проект' }).click();
    const dialog = page.getByRole('dialog', { name: 'Удалить проект «Web»?' });
    await expect
      .element(dialog.getByText(/^В проекте 3\u00a0задачи\. Все задачи и статьи/))
      .toBeVisible();
    if (isPhone()) expect(smallTargets(dialog.element())).toEqual([]);
    await dialog.getByRole('button', { name: 'Отмена' }).click();

    fails = true;
    await screen.getByRole('button', { name: 'Удалить проект' }).click();
    await expect.element(dialog.getByText(/^Все задачи и статьи проекта/)).toBeVisible();
    expect(dialog.getByText(/В проекте/).elements()).toEqual([]);
  });

  it('PRJ-064: without the permission to delete there is no deletion; Russian texts', async () => {
    setLocale('ru');
    server(MANAGER, { 'GET /projects/by-key/WEB': { body: WEB } });
    const screen = await settingsPage(WEB);

    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Общие настройки' }))
      .toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Архивировать проект' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Удалить проект' }).query()).toBeNull();
    await expect
      .element(
        screen
          .getByRole('group', { name: 'Видимость по умолчанию' })
          .getByText('Все, кто видит проект'),
      )
      .toBeVisible();
    await expectAccessible(screen.container);
  });
});

describe('SettingsNav', () => {
  it('PRJ-037: the settings pages, the current one marked', async () => {
    setAccessForTests(MANAGER);
    const screen = await render(SettingsNav, {
      project: WEB,
      pathname: '/projects/WEB/settings/general',
    });
    const nav = screen.getByRole('navigation', { name: 'Project settings' });
    expect(
      nav
        .getByRole('link')
        .elements()
        .map((link) => link.textContent.trim()),
    ).toEqual(['General', 'People', 'Fields', 'Rules']);
    await expect
      .element(nav.getByRole('link', { name: 'General' }))
      .toHaveAttribute('aria-current', 'page');
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('PRJ-009, PRJ-046: people who read the project in full see only the people page', async () => {
    setAccessForTests({
      global: [],
      projects: [{ project_id: 1, permissions: ['project.basic', 'project.full'] }],
      capabilities: [],
    });
    const screen = await render(SettingsNav, {
      project: WEB,
      pathname: '/projects/WEB/settings/people',
    });
    const nav = screen.getByRole('navigation', { name: 'Project settings' });
    expect(
      nav
        .getByRole('link')
        .elements()
        .map((link) => link.textContent.trim()),
    ).toEqual(['People']);
  });
});
