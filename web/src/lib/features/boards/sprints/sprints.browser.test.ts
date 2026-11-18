// Sprints on the board screen (step 1.8b-5; BRD-017, BRD-070, BRD-081, BRD-083, BRD-084,
// BRD-098…BRD-104).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { BoardView } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import BoardPage from '../board/BoardPage.svelte';
import { BoardState } from '../board/board-state.svelte.ts';
import { headerFilter } from '../board/header-filter.svelte.ts';
import { MEMBER, SESSION, board, sprint, view } from '../test-data.ts';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

const S1 = sprint(11, 'Sprint 1', {
  current: true,
  starts_on: '2026-10-12',
  ends_on: '2026-10-25',
  goal: 'Ship the login',
});
const S2 = sprint(12, 'Sprint 2', { starts_on: '2026-10-26', ends_on: '2026-11-08' });
const OLD = sprint(10, 'Sprint 0', {
  archived: true,
  finished: true,
  starts_on: '2026-09-28',
  ends_on: '2026-10-11',
});
const SCRUM = board({ preset: 'scrum', sprints_on: true });
/** The last day of Sprint 1, at noon in every zone near UTC. */
const LAST_DAY = Date.parse('2026-10-25T10:00:00Z');

function scrumView(extra: Partial<BoardView> = {}): BoardView {
  return view({ board: SCRUM, sprint: S1, sprints: [S1, S2], ...extra });
}

function server(routes: Record<string, FakeRoute> = {}, shown = scrumView()): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({
    'GET /boards/9/view': { body: shown },
    'GET /boards/9/sprints': { body: { items: [OLD, S1, S2] } },
    ...routes,
  });
}

const views = (calls: FakeRequest[]) => calls.filter((call) => call.path === '/boards/9/view');

beforeEach(() => {
  navigation.goto.mockClear();
  headerFilter.clear();
  toasts.clear();
});

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-083: the sprint menu and the schedule', () => {
  it('lists the sprints newest first, the current in bold, the archived apart', async () => {
    server();
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });

    await screen.getByRole('button', { name: 'Sprint: Sprint 1' }).click();
    const list = screen.getByRole('list', { name: 'Sprint', exact: true });
    const links = list.getByRole('link').elements();
    expect(links.map((one) => one.querySelector('span')?.textContent)).toEqual([
      'Sprint 2',
      'Sprint 1current',
    ]);
    expect(links[1]?.classList.contains('current')).toBe(true);
    expect(links[1]?.getAttribute('aria-current')).toBe('page');
    expect(links[0]?.getAttribute('href')).toBe('/boards/9/sprints/12');
    const archived = screen.getByRole('list', { name: 'Archived' });
    await expect.element(archived.getByRole('link', { name: /Sprint 0/ })).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Edit “Sprint 2”' })).toBeVisible();
    await expectAccessible(screen.container);

    await screen.getByRole('searchbox', { name: 'Find a sprint' }).fill('0');
    await expect.element(screen.getByRole('link', { name: /Sprint 2/ })).not.toBeInTheDocument();
    await expect.element(archived.getByRole('link', { name: /Sprint 0/ })).toBeVisible();
  });

  it('shows the dates and, on the last day, only the flag', async () => {
    server();
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    const schedule = screen.getByRole('group', { name: 'Schedule' });
    await expect.element(schedule.getByText('Last day')).toBeVisible();
    await expect.element(schedule.getByRole('button', { name: 'Finish' })).toBeVisible();
  });

  it('counts the days left and shows nothing to edit for a reader', async () => {
    const reader = board({
      preset: 'scrum',
      sprints_on: true,
      rights: { delete: false, edit: false, share: false, transfer: false },
    });
    server({}, scrumView({ board: reader }));
    const screen = await render(BoardPage, {
      board: reader,
      now: Date.parse('2026-10-20T10:00:00Z'),
    });
    const schedule = screen.getByRole('group', { name: 'Schedule' });
    await expect.element(schedule.getByText('5 days left')).toBeVisible();
    expect(schedule.getByRole('button').elements()).toEqual([]);
    await screen.getByRole('button', { name: 'Sprint: Sprint 1' }).click();
    await expect.element(screen.getByRole('link', { name: /Sprint 2/ })).toBeVisible();
    expect(screen.getByRole('button', { name: /Edit “/ }).elements()).toEqual([]);
    expect(screen.getByRole('button', { name: 'New sprint' }).elements()).toEqual([]);
  });

  it('BRD-017: a chosen sprint opens at its address and loads its view', async () => {
    const calls = server({
      'GET /boards/9/view': (request) => ({
        body: request.query.get('sprint') === '12' ? scrumView({ sprint: S2 }) : scrumView(),
      }),
    });
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    await screen.getByRole('button', { name: 'Sprint: Sprint 1' }).click();
    await screen.getByRole('link', { name: /Sprint 2/ }).click();
    expect(navigation.goto).toHaveBeenCalledWith('/boards/9/sprints/12');
    await expect.element(screen.getByRole('button', { name: 'Sprint: Sprint 2' })).toBeVisible();
    expect(views(calls).at(-1)?.query.get('sprint')).toBe('12');
  });

  it('shows no sprint menu on a board without sprints', async () => {
    server({}, view());
    const screen = await render(BoardPage, { board: board() });
    await expect.element(screen.getByRole('region', { name: 'Columns' })).toBeVisible();
    expect(screen.getByRole('button', { name: /^Sprint:/ }).elements()).toEqual([]);
  });
});

describe('BRD-098: a new sprint', () => {
  it('asks for a name, carries the current sprint over and opens the new one', async () => {
    const S3 = sprint(13, 'Sprint 3');
    const calls = server({ 'POST /boards/9/sprints': { status: 201, body: S3 } });
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    await screen.getByRole('button', { name: 'Sprint: Sprint 1' }).click();
    await screen.getByRole('button', { name: 'New sprint' }).click();

    const dialog = screen.getByRole('dialog', { name: 'New sprint' });
    await dialog.getByRole('button', { name: 'Create' }).click();
    await expect.element(dialog.getByText('Give the sprint a name.')).toBeVisible();

    await dialog.getByRole('textbox', { name: 'Name' }).fill('Sprint 3');
    await dialog
      .getByRole('checkbox', { name: 'Carry the unresolved issues of “Sprint 1” over' })
      .click();
    await dialog.getByRole('checkbox', { name: 'New issues join this sprint' }).click();
    await expectAccessible(screen.container);
    await dialog.getByRole('button', { name: 'Create' }).click();

    await expect.poll(() => navigation.goto.mock.calls.at(-1)).toEqual(['/boards/9/sprints/13']);
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({
      name: 'Sprint 3',
      goal: '',
      starts_on: null,
      ends_on: null,
      carry_from: 11,
      auto_target: true,
    });
    expect(toasts.items[0]?.message).toBe('Sprint “Sprint 3” created');
  });
});

describe('BRD-102: editing a sprint', () => {
  it('opens from the schedule with the sprint filled in and saves against its version', async () => {
    const calls = server({
      'PATCH /boards/9/sprints/11': (request) => ({
        body: { ...S1, ...(request.body as object), version: 2 },
      }),
    });
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    await screen.getByRole('button', { name: /^Schedule:/ }).click();
    const dialog = screen.getByRole('dialog', { name: 'Edit sprint' });
    const name = dialog.getByRole('textbox', { name: 'Name' });
    await expect.element(name).toHaveValue('Sprint 1');
    await expect
      .element(dialog.getByRole('textbox', { name: 'Goal' }))
      .toHaveValue('Ship the login');
    // The carry and auto-add options belong to a new sprint.
    expect(dialog.getByRole('checkbox').elements()).toEqual([]);
    await name.fill('Sprint 1b');
    await dialog.getByRole('button', { name: 'Save' }).click();

    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.find((call) => call.method === 'PATCH')?.body).toEqual({
      name: 'Sprint 1b',
      goal: 'Ship the login',
      starts_on: '2026-10-12',
      ends_on: '2026-10-25',
      version: 1,
    });
    await expect.poll(() => views(calls).length).toBe(2);
  });

  it('says when the sprint changed meanwhile', async () => {
    server({
      'PATCH /boards/9/sprints/11': { status: 409, body: { code: 'conflict', message: 'x' } },
    });
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    await screen.getByRole('button', { name: /^Schedule:/ }).click();
    const dialog = screen.getByRole('dialog', { name: 'Edit sprint' });
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect
      .element(dialog.getByText('Someone changed the sprint meanwhile. Check it and save again.'))
      .toBeVisible();
  });
});

describe('BRD-100, BRD-101: finishing a sprint', () => {
  it('finishes it and carries the unresolved issues to the chosen sprint', async () => {
    const calls = server({
      'POST /boards/9/sprints/11/finish': { body: { ...S1, finished: true } },
      'POST /boards/9/sprints/11/carry': { body: S2 },
    });
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    await screen.getByRole('button', { name: 'Finish' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Finish “Sprint 1”?' });
    await dialog.getByRole('radio', { name: 'Move to another sprint' }).click();
    await expect.element(dialog.getByRole('combobox', { name: 'Sprint' })).toHaveValue('12');
    await dialog.getByRole('button', { name: 'Finish' }).click();

    await expect.element(dialog).not.toBeInTheDocument();
    const posts = calls.filter((call) => call.method === 'POST').map((call) => call.path);
    expect(posts).toEqual(['/boards/9/sprints/11/finish', '/boards/9/sprints/11/carry']);
    expect(calls.find((call) => call.path.endsWith('/carry'))?.body).toEqual({ to: 12 });
    expect(toasts.items[0]?.message).toBe('Sprint finished; unresolved issues moved to “Sprint 2”');
  });

  it('starts a sprint by hand', async () => {
    const calls = server(
      { 'POST /boards/9/sprints/12/start': { body: { ...S2, starts_on: '2026-10-25' } } },
      scrumView({ sprint: S2 }),
    );
    const screen = await render(BoardPage, { board: SCRUM, sprint: 12, now: LAST_DAY });
    const schedule = screen.getByRole('group', { name: 'Schedule' });
    await expect.element(schedule.getByText('Starts tomorrow')).toBeVisible();
    await schedule.getByRole('button', { name: 'Start' }).click();
    await expect.poll(() => toasts.items[0]?.message).toBe('Sprint started');
    expect(calls.some((call) => call.path === '/boards/9/sprints/12/start')).toBe(true);
  });
});

describe('BRD-084, BRD-103, BRD-104: the footer and archived sprints', () => {
  it('shows the owner and the goal of the sprint', async () => {
    server();
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    const footer = screen.getByRole('contentinfo', { name: 'About the board' });
    await expect.element(footer.getByText('Ship the login')).toBeVisible();
    await expect.element(footer.getByText('Anna Admin')).toBeVisible();
  });

  it('archives a sprint from its dialog', async () => {
    const calls = server({
      'POST /boards/9/sprints/12/archive': { body: { ...S2, archived: true } },
    });
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    await screen.getByRole('button', { name: 'Sprint: Sprint 1' }).click();
    await screen.getByRole('button', { name: 'Edit “Sprint 2”' }).click();
    await screen
      .getByRole('dialog', { name: 'Edit sprint' })
      .getByRole('button', { name: 'Archive' })
      .click();
    await expect.poll(() => toasts.items[0]?.message).toBe('Sprint archived');
    expect(calls.some((call) => call.path === '/boards/9/sprints/12/archive')).toBe(true);
  });

  it('restores an archived sprint shown on the board', async () => {
    const calls = server(
      { 'POST /boards/9/sprints/10/restore': { body: { ...OLD, archived: false } } },
      scrumView({ sprint: OLD }),
    );
    const screen = await render(BoardPage, { board: SCRUM, sprint: 10, now: LAST_DAY });
    const footer = screen.getByRole('contentinfo', { name: 'About the board' });
    await expect.element(footer.getByText('This sprint is in the archive.')).toBeVisible();
    await footer.getByRole('button', { name: 'Restore' }).click();
    await expect.poll(() => toasts.items[0]?.message).toBe('Sprint restored');
    expect(calls.some((call) => call.path === '/boards/9/sprints/10/restore')).toBe(true);
  });

  it('deletes an archived sprint after a confirmation and opens the current one', async () => {
    const calls = server(
      { 'DELETE /boards/9/sprints/10': { status: 204 } },
      scrumView({ sprint: OLD }),
    );
    const screen = await render(BoardPage, { board: SCRUM, sprint: 10, now: LAST_DAY });
    const footer = screen.getByRole('contentinfo', { name: 'About the board' });
    await footer.getByRole('button', { name: 'Delete sprint' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Delete “Sprint 0”?' });
    await expect.element(dialog.getByText(/This cannot be undone/)).toBeVisible();
    await dialog.getByRole('button', { name: 'Delete sprint' }).click();
    await expect.poll(() => navigation.goto.mock.calls.at(-1)).toEqual(['/boards/9']);
    expect(calls.some((call) => call.method === 'DELETE')).toBe(true);
  });
});

describe('BRD-081: the header filter', () => {
  it('sends the query with the view, puts it in the address and keeps it', async () => {
    const calls = server();
    const screen = await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    const bar = screen.getByRole('search', { name: 'Card filter' });
    await bar.getByRole('combobox').fill('for: me');
    await bar.getByRole('button', { name: 'Find' }).click();

    await expect.poll(() => views(calls).at(-1)?.query.get('q')).toBe('for: me');
    expect(navigation.goto).toHaveBeenCalledWith('/boards/9?q=for%3A%20me', {
      replace: true,
      reset: false,
    });
    expect(headerFilter.value).toBe('for: me');
  });

  it('BRD-070: a filter kept from another board joins this one', async () => {
    headerFilter.set('#Bug');
    const calls = server();
    await render(BoardPage, { board: SCRUM, now: LAST_DAY });
    await expect.poll(() => views(calls)[0]?.query.get('q')).toBe('#Bug');
    expect(navigation.goto).toHaveBeenCalledWith('/boards/9?q=%23Bug', {
      replace: true,
      reset: false,
    });
  });

  it('BRD-070: a sort clause orders the cells and refuses reordering by hand', async () => {
    const sorted = scrumView({ sort: [{ key: 'created', direction: 'desc', field: null }] });
    const calls = server({}, sorted);
    const screen = await render(BoardPage, { board: SCRUM, q: 'sort by: created', now: LAST_DAY });
    await expect.element(screen.getByText('Ordered by the query')).toBeVisible();

    const model = new BoardState(9, 11, 'sort by: created');
    await model.load();
    await model.move(101, { column: 1, index: 1 });
    expect(toasts.items[0]?.message).toBe(
      'The filter sorts the cards: they cannot be reordered within a column.',
    );
    expect(calls.some((call) => call.path.endsWith('/move'))).toBe(false);
  });
});
