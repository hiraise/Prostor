// The settings panel of a board on a desktop and on a phone (step 1.8a-5; BRD-016, BRD-042…
// BRD-046, BRD-086).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { Board, ProjectField, SavedSearch, ValueSet } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { errorText } from '#lib/i18n/messages/errors.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import BoardPage from '../board/BoardPage.svelte';
import {
  BORIS,
  DOING,
  DONE,
  MEMBER,
  OPEN,
  PRIORITY,
  SESSION,
  STATE,
  WEB,
  board,
  sprint,
  value,
  view,
} from '../test-data.ts';
import SettingsPanel from './SettingsPanel.svelte';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

const REVIEW = value(54, 'Review');

/** A saved search as a listing shows it; the backlog tab reads its ID and name. */
function savedSearch(id: number, name: string): SavedSearch {
  return {
    id,
    name,
    owner: BORIS,
    query: 'project: WEB',
    problems: [],
    builtin: null,
    project: null,
    sharing: null,
    abilities: { apply: true, edit: true, delete: true, share: true, transfer: true },
    version: 1,
    created_at: '2026-10-20T09:00:00Z',
    updated_at: '2026-10-20T09:00:00Z',
  };
}

function projectField(field: ProjectField['field'], set: number): ProjectField {
  return {
    aliases: [],
    can_be_empty: true,
    default_users: [],
    default_values: [],
    empty_label: '',
    field,
    field_default: true,
    private: false,
    required: false,
    set: { id: set, kind: field.kind === 'state' ? 'state' : 'enum', name: field.name },
    shared_with: 1,
    version: 1,
  };
}

const STATES: ValueSet = {
  id: 20,
  name: 'States',
  kind: 'state',
  created_at: '2026-10-20T09:00:00Z',
  editable: true,
  field_count: 1,
  groups: [],
  users: [],
  used_in: { items: [WEB, { ...WEB, id: 2, name: 'Mobile', key: 'MOB' }], hidden: 1 },
  values: [OPEN, DOING, DONE, REVIEW].map((one, index) => ({
    id: one.id,
    name: one.name,
    archived: false,
    assembled_on: null,
    color: null,
    description: '',
    owner: null,
    release_date: null,
    released: false,
    resolved: one.resolved,
    start_date: null,
    version: index + 1,
  })),
  version: 1,
};

function server(routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({
    'GET /projects/1/fields': {
      body: { items: [projectField(STATE, 20), projectField(PRIORITY, 30)] },
    },
    'GET /value-sets/20': { body: STATES },
    ...routes,
  });
}

async function panel(shown: Board = board()) {
  const onchanged = vi.fn();
  const screen = await render(SettingsPanel, { open: true, board: shown, onchanged });
  return { screen, onchanged };
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  store.entities.table('value_set').remove(20);
  navigation.goto.mockClear();
});

describe('BRD-086: «Основное»', () => {
  it('opens by itself on a new board and saves the name against the version', async () => {
    const calls = server({
      'GET /boards/9/view': { body: view() },
      'PATCH /boards/9': (request) => ({
        body: board({ name: (request.body as { name: string }).name, version: 4 }),
      }),
    });
    const screen = await render(BoardPage, { board: board(), openSettings: true });
    const dialog = screen.getByRole('dialog', { name: 'Board settings' });
    await expect
      .element(dialog.getByRole('tab', { name: 'General' }))
      .toHaveAttribute('aria-selected', 'true');
    await dialog.getByRole('textbox', { name: 'Name' }).fill('Team board');
    await dialog.getByRole('button', { name: 'Save' }).first().click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ name: 'Team board', version: 3 });
    await expectAccessible(screen.container.ownerDocument.body);
  });

  it('a stale version reloads the board and asks to repeat', async () => {
    const calls = server({
      'PATCH /boards/9': failure(409, 'conflict'),
      'GET /boards/9': { body: board({ version: 5 }) },
    });
    const { screen } = await panel();
    await screen.getByRole('textbox', { name: 'Name' }).fill('Other');
    await screen.getByRole('button', { name: 'Save' }).first().click();
    await expect
      .element(screen.getByText('The settings have changed meanwhile. Try again.'))
      .toBeVisible();
    expect(calls.some((call) => call.method === 'GET' && call.path === '/boards/9')).toBe(true);
  });

  it('gives the board to someone else', async () => {
    const calls = server({
      'GET /users': {
        body: {
          items: [
            {
              id: 2,
              username: 'boris',
              full_name: 'Boris Lead',
              email: null,
              avatar_color: 1,
              avatar_url: null,
              disabled: false,
              created_at: '2026-10-20T09:00:00Z',
              email_verified: true,
              full_profile: true,
              groups: [],
              guest: false,
              invited: false,
              projects: [],
            },
          ],
          next_cursor: null,
        },
      },
      'PATCH /boards/9': { body: board({ version: 4 }) },
    });
    const { screen } = await panel();
    await screen.getByRole('button', { name: 'Give to someone else' }).click();
    const dialog = screen.getByRole('dialog', { name: 'New owner of the board' });
    await dialog.getByRole('button', { name: 'Give' }).click();
    await expect.element(dialog.getByText('Choose a person.')).toBeVisible();
    await dialog.getByRole('searchbox').fill('bor');
    await dialog.getByRole('radio', { name: /Boris Lead/ }).click();
    await dialog.getByRole('button', { name: 'Give' }).click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ owner: 2, version: 3 });
  });

  it('BRD-016: deletes after saying what goes with the board', async () => {
    const calls = server({
      'GET /boards/9/impact': { body: { sprints: 2, cards: 14, searches: 0 } },
      'DELETE /boards/9': { status: 204 },
    });
    const { screen } = await panel();
    await screen.getByRole('button', { name: 'Delete board' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Delete “Team”?' });
    await expect
      .element(dialog.getByText(/2 sprints and 14 cards go with it\. The issues stay\./))
      .toBeVisible();
    await dialog.getByRole('button', { name: 'Delete board' }).click();
    await expect.poll(() => calls.some((call) => call.method === 'DELETE')).toBe(true);
    await expect.poll(() => navigation.goto.mock.calls.at(-1)).toEqual(['/boards?list']);
  });

  it('hides sharing, giving away and deleting from those who may only edit', async () => {
    server({});
    const { screen } = await panel(
      board({
        sharing: null,
        rights: { edit: true, share: false, transfer: false, delete: false },
      }),
    );
    await expect.element(screen.getByRole('textbox', { name: 'Name' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Delete board' }).elements()).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Give to someone else' }).elements()).toHaveLength(0);
    expect(screen.getByText('Access').elements()).toHaveLength(0);
  });
});

describe('BRD-042…BRD-046: «Колонки»', () => {
  async function columnsTab() {
    const opened = await panel();
    await opened.screen.getByRole('tab', { name: 'Columns' }).click();
    return opened;
  }

  it('offers single-value fields of the board projects', async () => {
    server({});
    const { screen } = await columnsTab();
    const select = screen.getByRole('combobox', { name: 'Field of the columns' });
    await expect.element(select).toHaveValue('5');
    expect(
      [...(select.element() as HTMLSelectElement).options].map((option) => option.text),
    ).toEqual(['Not chosen', 'Priority', 'State']);
    await expectAccessible(screen.container.ownerDocument.body);
  });

  it('changing the field asks first, since the columns go', async () => {
    const calls = server({
      'PATCH /boards/9': { body: board({ column_field: PRIORITY, columns: [] }) },
    });
    const { screen, onchanged } = await columnsTab();
    await screen.getByRole('combobox', { name: 'Field of the columns' }).selectOptions('7');
    const dialog = screen.getByRole('dialog', { name: 'Change the field?' });
    await expect.element(dialog.getByText(/All 3 columns go/)).toBeVisible();
    await dialog.getByRole('button', { name: 'Change' }).click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ column_field: 7, version: 3 });
    await expect.poll(() => onchanged.mock.calls.length).toBe(1);
  });

  it('adds a column from a value of the set, or a new value after naming the projects', async () => {
    const calls = server({ 'POST /boards/9/columns': { body: board() } });
    const { screen, onchanged } = await columnsTab();
    const existing = screen.getByRole('combobox', { name: 'Value of the field' });
    await expect.poll(() => (existing.element() as HTMLSelectElement).options.length).toBe(2);
    await existing.selectOptions('54');
    await screen.getByRole('button', { name: 'Add' }).first().click();
    await expect.poll(() => calls.at(-1)?.body).toEqual({ value: 54 });

    await screen.getByRole('textbox', { name: 'Or a new value' }).fill('Blocked');
    await screen.getByRole('button', { name: 'Add' }).nth(1).click();
    const dialog = screen.getByRole('dialog', { name: 'Add the value “Blocked”?' });
    await expect
      .element(
        dialog.getByText('It appears in every project using the set: Web, Mobile and 1 more.'),
      )
      .toBeVisible();
    await dialog.getByRole('button', { name: 'Add' }).click();
    await expect.poll(() => calls.at(-1)?.body).toEqual({ new_value: 'Blocked' });
    await expect.poll(() => onchanged.mock.calls.length).toBe(2);
  });

  it('moves, limits, renames and removes a column', async () => {
    const calls = server({
      'PATCH /boards/9/columns/2': { body: board() },
      'DELETE /boards/9/columns/3': { body: board() },
    });
    const { screen } = await columnsTab();

    await screen.getByRole('button', { name: 'Actions of “In Progress”' }).click();
    await screen.getByRole('menuitem', { name: 'Move up' }).click();
    await expect.poll(() => calls.at(-1)?.body).toEqual({ after: null });

    const max = screen.getByRole('spinbutton', { name: 'Limit of “In Progress”' });
    await max.fill('4');
    (max.element() as HTMLInputElement).dispatchEvent(new Event('change', { bubbles: true }));
    await expect.poll(() => calls.at(-1)?.body).toEqual({ wip_max: 4 });

    const min = screen.getByRole('spinbutton', { name: 'Minimum of “In Progress”' });
    await min.fill('9');
    (min.element() as HTMLInputElement).dispatchEvent(new Event('change', { bubbles: true }));
    await expect.element(screen.getByText(/the minimum is not above the limit/)).toBeVisible();

    await screen.getByRole('button', { name: 'Actions of “In Progress”' }).click();
    await screen.getByRole('menuitem', { name: 'Rename' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Rename “In Progress”' });
    await expect
      .element(dialog.getByText(/changes in every project using its set: Web, Mobile and 1 more/))
      .toBeVisible();
    await dialog.getByRole('textbox', { name: 'New name' }).fill('Doing');
    await dialog.getByRole('button', { name: 'Rename' }).click();
    await expect.poll(() => calls.at(-1)?.body).toEqual({ name: 'Doing' });

    await screen.getByRole('button', { name: 'Actions of “Done”' }).click();
    await screen.getByRole('menuitem', { name: 'Remove column' }).click();
    await expect.poll(() => calls.at(-1)?.method).toBe('DELETE');
  });

  it('says what the server refused', async () => {
    server({ 'POST /boards/9/columns': failure(403, 'forbidden') });
    const { screen } = await columnsTab();
    await screen.getByRole('textbox', { name: 'Or a new value' }).fill('Blocked');
    await screen.getByRole('button', { name: 'Add' }).nth(1).click();
    await screen
      .getByRole('dialog', { name: 'Add the value “Blocked”?' })
      .getByRole('button', { name: 'Add' })
      .click();
    await expect.element(screen.getByText(errorText.forbidden())).toBeVisible();
  });
});

describe('BRD-024…BRD-029: sprints and how issues join («Основное»)', () => {
  it('turns sprints on at once', async () => {
    const calls = server({ 'PATCH /boards/9': { body: board({ sprints_on: true, version: 4 }) } });
    const { screen, onchanged } = await panel();
    await screen.getByRole('checkbox', { name: 'Work in sprints' }).click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ sprints_on: true, version: 3 });
    await expect.poll(() => onchanged.mock.calls.length).toBe(1);
  });

  it('switches to a query board and saves its query (BRD-028, BRD-029)', async () => {
    const bodies: unknown[] = [];
    server({
      'PATCH /boards/9': (request) => {
        bodies.push(request.body);
        return { body: board({ mode: 'query', query: '', version: 4 }) };
      },
    });
    const { screen } = await panel();
    await screen.getByRole('radio', { name: /By a query/ }).click();
    await expect.poll(() => bodies).toEqual([{ mode: 'query', version: 3 }]);
  });

  it('shows the query of a query board with what it lost', async () => {
    const calls = server({ 'PATCH /boards/9': { body: board({ version: 4 }) } });
    const { screen } = await panel(
      board({
        mode: 'query',
        query: 'Assignee: me',
        query_problems: [{ code: 'deleted_reference', from: 0, to: 8, severity: 'error' }],
      }),
    );
    await expect
      .element(screen.getByText(/the board shows no cards until it is fixed/))
      .toBeVisible();
    const bar = screen.getByRole('search', { name: 'Board query' });
    await expect.element(bar.getByRole('combobox')).toHaveValue('Assignee: me');
    await bar.getByRole('combobox').fill('Assignee: boris');
    await bar.getByRole('button', { name: 'Find' }).click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ query: 'Assignee: boris', version: 3 });
  });

  it('chooses the sprint new issues join on an auto sprint board (BRD-026)', async () => {
    const calls = server({ 'PATCH /boards/9': { body: board({ version: 4 }) } });
    const scrum = board({ preset: 'scrum', sprints_on: true, mode: 'auto' });
    const onchanged = vi.fn();
    const screen = await render(SettingsPanel, {
      open: true,
      board: scrum,
      sprints: [sprint(11, 'Sprint 1', { current: true }), sprint(12, 'Sprint 2')],
      onchanged,
    });
    await screen.getByRole('combobox', { name: 'New issues join' }).selectOptions('Sprint 2');
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ auto_sprint: 12, version: 3 });
  });
});

describe('BRD-089, BRD-090: «Бэклог»', () => {
  const MINE = { id: 5, name: 'Team · backlog' };
  const OTHER = { id: 6, name: 'All web bugs' };

  function searchesServer(routes: Record<string, FakeRoute> = {}): FakeRequest[] {
    return server({
      'GET /saved-searches': {
        body: {
          items: [MINE, OTHER].map((one) => savedSearch(one.id, one.name)),
          next_cursor: null,
        },
      },
      ...routes,
    });
  }

  it('switches the backlog to another saved search', async () => {
    const calls = searchesServer({
      'PATCH /boards/9': { body: board({ backlog: 6, version: 4 }) },
    });
    const { screen, onchanged } = await panel(board({ backlog: 5 }));
    await screen.getByRole('tab', { name: 'Backlog' }).click();
    await expect
      .element(screen.getByText('The backlog is the saved search “Team · backlog”.'))
      .toBeVisible();
    await screen.getByRole('combobox', { name: 'Saved search' }).selectOptions('All web bugs');
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ backlog: 6, version: 3 });
    await expect.poll(() => onchanged.mock.calls.length).toBe(1);
    await expectAccessible(screen.container.ownerDocument.body);
  });

  it('shows why a search does not fit', async () => {
    searchesServer({
      'PATCH /boards/9': {
        status: 400,
        body: {
          code: 'invalid_request',
          message: 'invalid',
          fields: [
            {
              path: 'backlog',
              problem: 'invalid',
              message: 'the search must name only projects of this board',
            },
          ],
        },
      },
    });
    const { screen } = await panel(board({ backlog: 5 }));
    await screen.getByRole('tab', { name: 'Backlog' }).click();
    await screen.getByRole('combobox', { name: 'Saved search' }).selectOptions('All web bugs');
    await expect
      .element(screen.getByRole('combobox', { name: 'Saved search' }))
      .toHaveAttribute('aria-invalid', 'true');
  });

  it('creates a new backlog for a board without one', async () => {
    const calls = searchesServer({
      'PATCH /boards/9': { body: board({ backlog: 7, version: 4 }) },
    });
    const { screen } = await panel();
    await screen.getByRole('tab', { name: 'Backlog' }).click();
    await expect.element(screen.getByRole('combobox', { name: 'Saved search' })).toHaveValue('');
    await screen.getByRole('button', { name: 'Create a new backlog' }).click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ new_backlog: true, version: 3 });
  });
});
