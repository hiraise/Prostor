// Creating a board on a desktop and on a phone (step 1.8a-4; BRD-003, UI-002).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Board, Project, Session, UserRef } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { NO_CARDS, NO_LANES } from '../test-data.ts';
import CreateBoardDialog from './CreateBoardDialog.svelte';
import CreateBoardForm from './CreateBoardForm.svelte';
import { newBoardDialog, openNewBoard } from './open.svelte.ts';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

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

const ANNA: UserRef = {
  id: 1,
  full_name: 'Anna Admin',
  username: 'anna',
  avatar_color: 0,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

/** Reads issues in projects 1 and 2; sees project 3 without its issues. */
const READER: AccessMap = {
  global: [],
  projects: [
    { project_id: 1, permissions: ['project.basic', 'issue.read'] },
    { project_id: 2, permissions: ['project.basic', 'issue.read'] },
    { project_id: 3, permissions: ['project.basic'] },
  ],
  capabilities: [],
};

function project(id: number, name: string, key: string): Project {
  return {
    id,
    name,
    key,
    archived_at: null,
    created_at: '2026-10-20T09:00:00Z',
    default_audience: null,
    deletion: null,
    description: '',
    description_html: '',
    favorite: false,
    former_keys: [],
    kind: 'standard',
    logo_color: id % 8,
    logo_url: null,
    next_number: 1,
    owner: ANNA,
    team: { id: 100 + id, name, builtin: null, team_of: id },
    team_size: 1,
    template: 'blank',
    version: 1,
  };
}

const PROJECTS = [project(1, 'Web', 'WEB'), project(2, 'Mobile', 'MOB'), project(3, 'Docs', 'DOC')];

function created(id: number, name: string): Board {
  return {
    id,
    name,
    column_field: null,
    columns: [],
    lanes: NO_LANES,
    cards: NO_CARDS,
    health: [],
    created_at: '2026-10-25T09:00:00Z',
    updated_at: '2026-10-25T09:00:00Z',
    favorite: false,
    mode: 'manual',
    backlog: null,
    auto_sprint: null,
    query: null,
    query_problems: [],
    owner: ANNA,
    preset: 'kanban',
    projects: [],
    rights: { delete: true, edit: true, share: true, transfer: true },
    sharing: null,
    sprints_on: false,
    version: 1,
  };
}

function server(routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(READER);
  return fakeApi({
    // In the person's teams: Web and Docs (Docs without its issues).
    'GET /projects': (request) => ({
      body: {
        items:
          request.query.get('in_teams') === 'true'
            ? PROJECTS.filter((one) => one.id !== 2)
            : PROJECTS,
        next_cursor: null,
      },
    }),
    'GET /boards': (request) => ({
      body: {
        items:
          request.query.get('q')?.toLowerCase() === 'team'
            ? [{ id: 1, name: 'Team', favorite: false, owner: ANNA, projects: [] }]
            : [],
        next_cursor: null,
        last_visited: null,
      },
    }),
    ...routes,
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
  navigation.goto.mockClear();
  newBoardDialog.open = false;
  store.entities.table('board').clear();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-003: the new-board form', () => {
  it('offers the presets with a line each and only projects whose issues the person reads', async () => {
    server({});
    const screen = await render(CreateBoardForm, { oncreated: vi.fn(), oncancel: vi.fn() });

    const kinds = screen.getByRole('group', { name: 'Kind of board' });
    await expect.element(kinds.getByRole('radio', { name: 'Scrum' })).toBeChecked();
    await expect.element(kinds.getByRole('radio', { name: 'Kanban' })).not.toBeChecked();
    await expect.element(kinds.getByText(/A continuous flow without sprints/)).toBeVisible();
    await expect.element(kinds.getByRole('radio', { name: 'Custom' })).toBeVisible();
    await expect.element(screen.getByRole('checkbox', { name: /Web/ })).toBeVisible();
    await expect.element(screen.getByRole('checkbox', { name: /Mobile/ })).toBeVisible();
    expect(screen.getByRole('checkbox', { name: /Docs/ }).elements()).toHaveLength(0);
    await expectAccessible(screen.container);
  });

  it('checks the name and the projects, and flags a name the person already sees', async () => {
    const calls = server({});
    const screen = await render(CreateBoardForm, { oncreated: vi.fn(), oncancel: vi.fn() });

    await screen.getByRole('button', { name: 'Create', exact: true }).click();
    await expect.element(screen.getByText('Enter a name.')).toBeVisible();
    await expect.element(screen.getByText('Choose at least one project.')).toBeVisible();
    expect(calls.some((call) => call.method === 'POST')).toBe(false);

    await screen.getByRole('textbox', { name: 'Name' }).fill('team');
    await expect.element(screen.getByText('You already see a board with this name.')).toBeVisible();
  });

  it('creates the board with the preset, the projects and the sharing', async () => {
    const calls = server({ 'POST /boards': { body: created(10, 'Flow') } });
    const oncreated = vi.fn();
    const screen = await render(CreateBoardForm, { oncreated, oncancel: vi.fn() });

    await screen.getByRole('radio', { name: 'Kanban' }).click();
    await screen.getByRole('textbox', { name: 'Name' }).fill('Flow');
    await screen.getByRole('checkbox', { name: /Mobile/ }).click();
    await screen.getByRole('button', { name: 'All my projects' }).click();
    await expect.element(screen.getByRole('checkbox', { name: /Web/ })).toBeChecked();
    await screen.getByRole('checkbox', { name: /reads issues in all its projects/ }).click();
    await screen.getByRole('button', { name: 'Create', exact: true }).click();

    await expect.poll(() => oncreated.mock.calls.length).toBe(1);
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({
      name: 'Flow',
      preset: 'kanban',
      projects: [2, 1],
      sharing: {
        view: { users: [], groups: [] },
        edit: { users: [], groups: [] },
        view_readers: true,
        edit_updaters: false,
      },
    });
    expect(store.entities.table('board').get(10)?.name).toBe('Flow');
  });

  it('shows the server’s refusal at the field it names', async () => {
    server({
      'POST /boards': failure(400, 'invalid_request', [{ path: 'name', code: 'too_long' }]),
    });
    const screen = await render(CreateBoardForm, {
      project: { id: 1, name: 'Web', key: 'WEB', logo_color: 1 },
      oncreated: vi.fn(),
      oncancel: vi.fn(),
    });
    await expect.element(screen.getByRole('checkbox', { name: /Web/ })).toBeChecked();
    await screen.getByRole('textbox', { name: 'Name' }).fill('Long');
    await screen.getByRole('button', { name: 'Create', exact: true }).click();
    await expect.element(screen.getByText('The value is too long.')).toBeVisible();
  });
});

describe('UI-002: opening the form', () => {
  it('opens a dialog on a desktop and the page on a phone; a new board opens with its settings', async () => {
    server({ 'POST /boards': { body: created(11, 'Ops') } });
    openNewBoard();
    if (isPhone()) {
      expect(navigation.goto).toHaveBeenCalledWith('/boards/new');
      return;
    }
    const screen = await render(CreateBoardDialog);
    const dialog = screen.getByRole('dialog', { name: 'New board' });
    await expect.element(dialog).toBeVisible();
    await dialog.getByRole('textbox', { name: 'Name' }).fill('Ops');
    await dialog.getByRole('checkbox', { name: /Web/ }).click();
    await dialog.getByRole('button', { name: 'Create', exact: true }).click();
    await expect.poll(() => navigation.goto.mock.calls.at(-1)).toEqual(['/boards/11?settings']);
    expect(newBoardDialog.open).toBe(false);
  });
});
