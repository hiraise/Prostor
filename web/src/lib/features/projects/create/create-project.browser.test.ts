// Creating a project on a desktop and on a phone (step 1.2; PRJ-002…PRJ-004, PRJ-006, PRJ-017…
// PRJ-019).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, NewProject, Project, Session } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { parseFirstNumber } from '../problems.ts';
import CreateProjectPage from './CreateProjectPage.svelte';

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

const CREATOR: AccessMap = { global: ['project.create'], projects: [], capabilities: [] };
const NOBODY: AccessMap = { global: [], projects: [], capabilities: [] };

function created(input: NewProject, key: string): Project {
  return {
    id: 9,
    name: input.name,
    key,
    archived_at: null,
    created_at: '2026-10-22T09:00:00Z',
    default_audience: null,
    deletion: null,
    description: input.description ?? '',
    description_html: '',
    favorite: false,
    former_keys: [],
    kind: 'standard',
    logo_color: 3,
    logo_url: null,
    next_number: input.first_number ?? 1,
    owner: null,
    team: null,
    team_size: 1,
    template: input.template,
    version: 1,
  };
}

/** Suggests the capitals of the name's words; `HR` and `WEB` are taken; keys must be valid. */
const check: FakeRoute = (request) => {
  const name = request.query.get('name');
  const key = request.query.get('key');
  const fields: { path: string; code: string; message: string }[] = [];
  if (name?.toLowerCase() === 'web') fields.push({ path: 'name', code: 'taken', message: 'x' });
  if (key !== null) {
    const upper = key.toUpperCase();
    if (!/^[A-Z][A-Z0-9_]*$/.test(upper))
      fields.push({ path: 'key', code: 'invalid', message: 'x' });
    else if (upper.length < 2) fields.push({ path: 'key', code: 'too_short', message: 'x' });
    else if (upper === 'HR' || upper === 'WEB')
      fields.push({ path: 'key', code: 'taken', message: 'x' });
  }
  const suggested = name
    ? name
        .split(/\s+/)
        .map((word) => word.charAt(0).toUpperCase())
        .join('')
        .padEnd(2, 'X')
    : null;
  return { body: { fields, suggested_key: suggested } };
};

function server(access: AccessMap, routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi({
    'GET /users/me/access': { body: access },
    'GET /projects/check': check,
    ...routes,
  });
}

/** A field by its label; required fields add "*" to the label text. */
function label(text: string): RegExp {
  return new RegExp(`^${text}\\s*\\*?$`);
}

afterEach(() => {
  vi.unstubAllGlobals();
  store.entities.table('project').clear();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
});

describe('CreateProjectPage', () => {
  it('PRJ-017, PRJ-018, PRJ-003: suggests the key from the name until the person types their own', async () => {
    const calls = server(CREATOR, {});
    const screen = await render(CreateProjectPage, { oncreated: vi.fn() });

    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'New project' }))
      .toBeVisible();
    const key = screen.getByLabelText(label('Key'));
    await screen.getByLabelText(label('Name')).fill('Mobile app');
    await expect.element(key).toHaveValue('MA');
    await expect
      .element(screen.getByText('Suggested from the name', { exact: false }))
      .toBeVisible();
    expect(
      calls
        .filter((call) => call.path === '/projects/check')
        .at(-1)
        ?.query.get('key'),
    ).toBeNull();

    // A key of one's own is kept while the name changes, and checked instead.
    await key.fill('app');
    await screen.getByLabelText(label('Name')).fill('Mobile application');
    await vi.waitFor(() => {
      const last = calls.filter((call) => call.path === '/projects/check').at(-1);
      expect(last?.query.get('key')).toBe('app');
      expect(last?.query.get('name')).toBe('Mobile application');
    });
    await expect.element(key).toHaveValue('app');
    await expect
      .element(screen.getByText('Starts every issue ID, such as APP-1.', { exact: false }))
      .toBeVisible();

    // Clearing it brings the suggestion back.
    await key.fill('');
    await expect.element(key).toHaveValue('MA');
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('PRJ-018: a taken name and a key that is not valid are said before creating', async () => {
    const calls = server(CREATOR, {});
    const screen = await render(CreateProjectPage, { oncreated: vi.fn() });

    await screen.getByLabelText(label('Name')).fill('Web');
    await expect.element(screen.getByText('Another project has this name.')).toBeVisible();
    await screen.getByLabelText(label('Key')).fill('1st');
    await expect
      .element(screen.getByText('Use Latin letters, digits and _, starting with a letter.'))
      .toBeVisible();
    await screen.getByLabelText(label('Key')).fill('hr');
    await expect.element(screen.getByText('Another project has or had this key.')).toBeVisible();
    await expect
      .element(screen.getByLabelText(label('Key')))
      .toHaveAttribute('aria-invalid', 'true');
    expect(calls.some((call) => call.method === 'POST')).toBe(false);
    await expectAccessible(screen.container);
  });

  it('PRJ-017, PRJ-019, PRJ-004, PRJ-006: creates the project from the chosen template and opens it', async () => {
    const calls = server(CREATOR, {
      'POST /projects': ({ body }) => ({ status: 201, body: created(body as NewProject, 'TB') }),
    });
    const oncreated = vi.fn();
    const screen = await render(CreateProjectPage, { oncreated });

    const templates = screen.getByRole('group', { name: 'Template' });
    await expect.element(templates.getByRole('radio', { name: 'Blank' })).toBeChecked();
    await expect.element(templates.getByText('Sprints, story points and ideal days')).toBeVisible();
    await expect.element(templates.getByText('Readiness: ready to pull or blocked')).toBeVisible();
    // The whole card chooses its template: the label covers it.
    await templates.getByText('Plan the work in sprints').click({ force: true });
    await expect.element(templates.getByRole('radio', { name: 'Scrum' })).toBeChecked();

    await screen.getByLabelText(label('Name')).fill('Team board');
    await expect.element(screen.getByLabelText(label('Key'))).toHaveValue('TB');
    await screen.getByLabelText(label('First issue number')).fill('100');
    await expect.element(screen.getByText('The first issue gets the ID TB-100.')).toBeVisible();
    await screen.getByLabelText(label('Description')).fill('Our sprints.\nAnd more.');
    await screen.getByRole('button', { name: 'Create project' }).click();

    await vi.waitFor(() => {
      expect(oncreated).toHaveBeenCalledOnce();
    });
    expect(oncreated.mock.calls[0]?.[0]).toMatchObject({ id: 9, key: 'TB', template: 'scrum' });
    // The suggested key is the server's to pick.
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({
      template: 'scrum',
      name: 'Team board',
      first_number: 100,
      description: 'Our sprints.\nAnd more.',
    });
    expect(store.entities.table('project').get(9)?.name).toBe('Team board');
  });

  it('PRJ-017: problems found on submitting are shown on their fields', async () => {
    let attempts = 0;
    const calls = server(CREATOR, {
      'POST /projects': ({ body }) =>
        ++attempts === 1
          ? failure(400, 'invalid_request', [{ path: 'key', code: 'taken', message: 'x' }])
          : { status: 201, body: created(body as NewProject, 'OPS') },
    });
    const oncreated = vi.fn();
    const screen = await render(CreateProjectPage, { oncreated });

    await screen.getByRole('button', { name: 'Create project' }).click();
    await expect.element(screen.getByText('Fill in this field.')).toBeVisible();
    await expect.element(screen.getByLabelText(label('Name'))).toHaveFocus();
    await screen.getByLabelText(label('First issue number')).fill('0');
    await screen.getByLabelText(label('Name')).fill('Operations');
    await screen.getByRole('button', { name: 'Create project' }).click();
    await expect
      .element(screen.getByText('Use a whole number from 1 to 1,000,000,000.'))
      .toBeVisible();
    expect(calls.some((call) => call.method === 'POST')).toBe(false);

    await screen.getByLabelText(label('First issue number')).fill('1');
    await screen.getByLabelText(label('Key')).fill('ops');
    // The key derived from the name may still be arriving: submit only once the field holds ours.
    await expect.element(screen.getByLabelText(label('Key'))).toHaveValue('ops');
    await screen.getByRole('button', { name: 'Create project' }).click();
    await expect.element(screen.getByText('Another project has or had this key.')).toBeVisible();
    await expect.element(screen.getByLabelText(label('Key'))).toHaveFocus();
    expect(calls.find((call) => call.method === 'POST')?.body).toMatchObject({ key: 'OPS' });
    expect(oncreated).not.toHaveBeenCalled();

    await screen.getByLabelText(label('Key')).fill('ops2');
    await screen.getByRole('button', { name: 'Create project' }).click();
    await vi.waitFor(() => {
      expect(oncreated).toHaveBeenCalledOnce();
    });
  });

  it('PRJ-017: in Russian, and forbidden without the permission to create projects', async () => {
    setLocale('ru');
    server(CREATOR, {});
    const screen = await render(CreateProjectPage, { oncreated: vi.fn() });
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Новый проект' }))
      .toBeVisible();
    await expect.element(screen.getByRole('radio', { name: 'Канбан' })).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Создать проект' })).toBeVisible();
    await expectAccessible(screen.container);
    await screen.unmount();

    setAccessForTests(NOBODY);
    const denied = await render(CreateProjectPage, { oncreated: vi.fn() });
    await expect.element(denied.getByRole('heading', { level: 1 })).toBeVisible();
    expect(denied.getByLabelText(label('Название')).query()).toBeNull();
  });
});

describe('parseFirstNumber', () => {
  it('PRJ-004: accepts whole numbers from 1 to a billion', () => {
    expect(parseFirstNumber(' 42 ')).toBe(42);
    expect(parseFirstNumber('1000000000')).toBe(1_000_000_000);
    expect(parseFirstNumber('0')).toBeNull();
    expect(parseFirstNumber('1000000001')).toBeNull();
    expect(parseFirstNumber('1.5')).toBeNull();
    expect(parseFirstNumber('-3')).toBeNull();
    expect(parseFirstNumber('')).toBeNull();
  });
});
