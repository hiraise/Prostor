// Lanes on the board and in the settings (step 1.8c-3; BRD-049…BRD-057): value and parent lanes,
// «Прочее», headers with collapse and order, lane pages, moves across lanes by the menu and by
// dragging, the «Дорожки» tab, lanes on a phone.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { Board, BoardView, Lane, ProjectField, ValueSet } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import BoardPage from '../board/BoardPage.svelte';
import SettingsPanel from '../settings/SettingsPanel.svelte';
import {
  MAJOR,
  MEMBER,
  NO_LANES,
  PRIORITY,
  SESSION,
  STATE,
  board,
  issue,
  value,
  view,
} from '../test-data.ts';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

const MINOR = value(72, 'Minor', { color: 5 });

function lane(key: string, count: number, extra: Partial<Lane> = {}): Lane {
  return { key, count, estimate: null, issue: null, ...extra };
}

/** Lanes by Priority: «Major» with two cards, «Minor» with none (named by `labels`), «Прочее». */
const BY_PRIORITY: Board['lanes'] = {
  mode: 'values',
  field: PRIORITY,
  attribute: null,
  values: ['v:71', 'v:72'],
  other: 'bottom',
  new_card_value: null,
  labels: [
    { key: 'v:71', name: 'Major', color: 3 },
    { key: 'v:72', name: 'Minor', color: 5 },
  ],
};

function laned(extra: Partial<BoardView> = {}, lanes: Board['lanes'] = BY_PRIORITY): BoardView {
  const plain = view();
  return {
    ...plain,
    board: board({ lanes }),
    cards: [
      { issue: 101, column: 1, lane: 'v:71', rank: 'a' },
      { issue: 102, column: 1, lane: 'other', rank: 'b' },
      { issue: 103, column: 2, lane: 'v:71', rank: 'a' },
      { issue: 104, column: 3, lane: 'other', rank: 'a' },
    ],
    cells: [],
    lanes: [lane('v:71', 2), lane('v:72', 0), lane('other', 2)],
    lanes_total: 3,
    ...extra,
  };
}

/** Two parent lanes and «Прочее»; the parents' cards come in `issues`. */
function parents(): BoardView {
  const plain = view();
  return laned(
    {
      cards: [
        { issue: 101, column: 1, lane: 'i:200', rank: 'a' },
        { issue: 102, column: 1, lane: 'other', rank: 'b' },
      ],
      issues: [...plain.issues, issue(200, 'Sign-in epic'), issue(201, 'Search epic')],
      lanes: [lane('i:200', 1, { issue: 200 }), lane('i:201', 0, { issue: 201 }), lane('other', 1)],
    },
    { ...NO_LANES, mode: 'parents', labels: [] },
  );
}

function server(routes: Record<string, FakeRoute>, shown: BoardView = laned()): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({ 'GET /boards/9/view': { body: shown }, ...routes });
}

async function page(shown: BoardView = laned()) {
  const screen = await render(BoardPage, {
    board: shown.board,
    live: { subscribe: () => () => undefined },
  });
  await expect.element(screen.getByRole('region', { name: 'Board lanes' })).toBeVisible();
  return screen;
}

function summaries(column: string, lane: string): string[] {
  const list = document.querySelector(`[aria-label="Cards of “${column}” in lane “${lane}”"]`);
  return [...(list?.querySelectorAll('article .summary') ?? [])].map((one) => one.textContent);
}

afterEach(() => {
  toasts.clear();
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  store.entities.table('value_set').remove(30);
});

describe('BRD-049, BRD-050, BRD-052: value lanes', () => {
  it('shows a row of cells per lane under the column headers, «Other» last', async () => {
    server({});
    const screen = await page();
    const lanes = screen.getByRole('region', { name: /^Lane “/ });
    expect(lanes.elements().map((one) => one.getAttribute('aria-label'))).toEqual([
      'Lane “Major”',
      'Lane “Minor”',
      'Lane “Other”',
    ]);
    expect(summaries('Open', 'Major')).toEqual(['Login form']);
    expect(summaries('Open', 'Other')).toEqual(['Logout']);
    expect(summaries('In Progress', 'Major')).toEqual(['Search']);
    // A lane without cards is named from the board's labels.
    const minor = screen.getByRole('region', { name: 'Lane “Minor”' });
    await expect.element(minor.getByText('0 cards')).toBeVisible();
    for (const name of ['Open', 'In Progress', 'Done']) {
      await expect.element(screen.getByRole('heading', { level: 2, name })).toBeVisible();
    }
    await expectAccessible(screen.container);
  });
});

describe('BRD-053: lane headers', () => {
  it.skipIf(isPhone())('collapses one lane, then all of them, for this person', async () => {
    const calls = server({
      'PATCH /boards/9/profile': (request) => ({
        body: { ...view().profile, ...(request.body as object) },
      }),
    });
    const screen = await page();
    await screen.getByRole('button', { name: 'Collapse lane “Major”' }).click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ collapsed_lanes: ['v:71'] });
    await vi.waitFor(() => {
      expect(document.querySelector('[aria-label^="Cards of “Open” in lane “Major”"]')).toBeNull();
    });
    await expect
      .element(screen.getByRole('button', { name: 'Expand lane “Major”' }))
      .toHaveAttribute('aria-expanded', 'false');

    await screen.getByRole('button', { name: 'Collapse all' }).click();
    await expect
      .poll(() => calls.filter((call) => call.method === 'PATCH').at(-1)?.body)
      .toEqual({ collapsed_lanes: ['v:71', 'v:72', 'other'] });
    await expect.element(screen.getByRole('button', { name: 'Expand all' })).toBeVisible();
  });

  it('moves a value lane down by saving the board’s order', async () => {
    const calls = server({
      'PATCH /boards/9': (request) => ({
        body: board({ ...(request.body as object), version: 4 }),
      }),
    });
    const screen = await page();
    await screen.getByRole('button', { name: 'Lane “Major”: actions' }).click();
    await screen.getByRole('menuitem', { name: 'Move down' }).click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({
        lanes: {
          mode: 'values',
          field: PRIORITY.id,
          attribute: null,
          values: ['v:72', 'v:71'],
          other: 'bottom',
          new_card_value: null,
        },
        version: 3,
      });
    // The view loads again with the new order.
    await expect.poll(() => calls.filter((call) => call.path === '/boards/9/view').length).toBe(2);
  });

  it('gives «Other» no menu: the settings place it', async () => {
    server({});
    const screen = await page();
    await expect
      .element(screen.getByRole('button', { name: 'Lane “Other”: actions' }))
      .not.toBeInTheDocument();
  });
});

describe('BRD-051, BRD-056: parent lanes', () => {
  it('names a parent lane by its issue and moves it after the next one', async () => {
    const calls = server({ 'POST /boards/9/sprints/7/lanes/200/move': { status: 204 } }, parents());
    const screen = await page(parents());
    const epic = screen.getByRole('region', { name: 'Lane “Sign-in epic”' });
    expect(epic.getByRole('link', { name: 'WEB-200' }).element().getAttribute('href')).toBe(
      '/issues/WEB-200',
    );
    expect(summaries('Open', 'Sign-in epic')).toEqual(['Login form']);

    await screen.getByRole('button', { name: 'Lane “Sign-in epic”: actions' }).click();
    await screen.getByRole('menuitem', { name: 'Move down' }).click();
    await expect
      .poll(() => calls.find((call) => call.path.endsWith('/lanes/200/move'))?.body)
      .toEqual({ after: 201 });
    await vi.waitFor(() => {
      const names = screen
        .getByRole('region', { name: /^Lane “/ })
        .elements()
        .map((one) => one.getAttribute('aria-label'));
      expect(names).toEqual(['Lane “Search epic”', 'Lane “Sign-in epic”', 'Lane “Other”']);
    });
  });

  it('takes a parent lane back to the backlog', async () => {
    const calls = server(
      { 'DELETE /boards/9/sprints/7/lanes/200': { body: { issues: [200, 101] } } },
      parents(),
    );
    const screen = await page(parents());
    await screen.getByRole('button', { name: 'Lane “Sign-in epic”: actions' }).click();
    await screen.getByRole('menuitem', { name: 'Back to the backlog' }).click();
    await vi.waitFor(() => {
      expect(toasts.items.some((one) => one.message === '2 issues went back to the backlog')).toBe(
        true,
      );
    });
    expect(calls.some((call) => call.method === 'DELETE')).toBe(true);
  });
});

describe('BRD-057: pages of lanes', () => {
  it('loads the next page of lanes with its cards', async () => {
    const first = laned({ lanes: [lane('v:71', 2)], lanes_next: '1', lanes_total: 3 });
    const next = laned({
      cards: [{ issue: 102, column: 1, lane: 'other', rank: 'b' }],
      issues: [issue(102, 'Logout')],
      lanes: [lane('v:72', 0), lane('other', 1)],
      lanes_next: null,
    });
    const calls = server({
      'GET /boards/9/view': (request) => ({
        body: request.query.get('lanes') === '1' ? next : first,
      }),
    });
    const screen = await page(first);
    // The end of the grid is in view, so the next page comes by itself; the button stays for
    // when it does not.
    await vi.waitFor(() => {
      expect(summaries('Open', 'Other')).toEqual(['Logout']);
    });
    expect(calls.some((call) => call.query.get('lanes') === '1')).toBe(true);
    await expect.element(screen.getByRole('region', { name: 'Lane “Minor”' })).toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: /^More lanes/ }))
      .not.toBeInTheDocument();
  });
});

describe('BRD-050: moving a card to another lane', () => {
  it('moves by the card menu and sends the lane', async () => {
    const calls = server({
      'POST /boards/9/sprints/7/cards/102/move': {
        body: {
          card: { issue: 102, column: 1, lane: 'v:72', rank: 'c' },
          issue: null,
          refs: { fields: [], values: [], users: [], groups: [] },
        },
      },
    });
    const screen = await page();
    await screen.getByRole('button', { name: 'Actions of WEB-102' }).click();
    await screen.getByRole('menuitem', { name: 'Minor' }).click();
    await vi.waitFor(() => {
      expect(summaries('Open', 'Minor')).toEqual(['Logout']);
    });
    expect(calls.find((call) => call.path.endsWith('/move'))?.body).toEqual({
      column: 1,
      lane: 'v:72',
      version: 1,
    });
    const minor = screen.getByRole('region', { name: 'Lane “Minor”' });
    await expect.element(minor.getByText('1 card')).toBeVisible();
  });

  it.skipIf(isPhone())('moves by dragging into a cell of another lane', async () => {
    const calls = server({
      'POST /boards/9/sprints/7/cards/102/move': {
        body: {
          card: { issue: 102, column: 2, lane: 'v:71', rank: 'b' },
          issue: null,
          refs: { fields: [], values: [], users: [], groups: [] },
        },
      },
    });
    await page();
    const card = document.querySelector<HTMLElement>('[data-card="102"] .summary');
    const cell = document.querySelector<HTMLElement>(
      '[data-drop-column="2"][data-drop-lane="v:71"]',
    );
    if (!card || !cell) throw new Error('no card or cell');
    const from = card.getBoundingClientRect();
    const to = cell.getBoundingClientRect();
    const point = { x: to.left + to.width / 2, y: to.bottom - 4 };
    const fire = (type: string, x: number, y: number) =>
      card.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          clientX: x,
          clientY: y,
          pointerId: 1,
          pointerType: 'mouse',
          isPrimary: true,
          button: 0,
        }),
      );
    const frame = () => new Promise((done) => requestAnimationFrame(done));
    fire('pointerdown', from.left + 10, from.top + 5);
    for (const step of [0.1, 0.5, 1]) {
      fire(
        'pointermove',
        from.left + 10 + (point.x - from.left - 10) * step,
        from.top + 5 + (point.y - from.top - 5) * step,
      );
      await frame();
    }
    await frame();
    fire('pointerup', point.x, point.y);
    await expect
      .poll(() => calls.find((call) => call.path.endsWith('/move'))?.body)
      .toEqual({ column: 2, lane: 'v:71', version: 1, after: 103 });
    await vi.waitFor(() => {
      expect(summaries('In Progress', 'Major')).toEqual(['Search', 'Logout']);
    });
  });
});

const PRIORITIES: ValueSet = {
  id: 30,
  name: 'Priorities',
  kind: 'enum',
  created_at: '2026-10-20T09:00:00Z',
  editable: true,
  field_count: 1,
  groups: [],
  users: [],
  used_in: { items: [], hidden: 0 },
  values: [MAJOR, MINOR].map((one, index) => ({
    id: one.id,
    name: one.name,
    archived: false,
    assembled_on: null,
    color: one.color,
    description: '',
    owner: null,
    release_date: null,
    released: false,
    resolved: false,
    start_date: null,
    version: index + 1,
  })),
  version: 1,
};

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
    set: { id: set, kind: 'enum', name: field.name },
    shared_with: 1,
    version: 1,
  };
}

function settingsServer(routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({
    'GET /projects/1/fields': {
      body: { items: [projectField(STATE, 20), projectField(PRIORITY, 30)] },
    },
    'GET /value-sets/30': { body: PRIORITIES },
    'PATCH /boards/9': (request) => ({ body: board({ ...(request.body as object), version: 4 }) }),
    ...routes,
  });
}

async function lanesTab(shown: Board) {
  const onchanged = vi.fn();
  const screen = await render(SettingsPanel, { open: true, board: shown, onchanged });
  await screen.getByRole('tab', { name: 'Lanes' }).click();
  return { screen, onchanged };
}

describe('BRD-086: «Дорожки»', () => {
  it('turns value lanes on once a field is chosen; the column field is not offered', async () => {
    const calls = settingsServer({});
    const { screen, onchanged } = await lanesTab(board());
    await screen.getByRole('radio', { name: /By the values of a field/ }).click();
    expect(calls.some((call) => call.method === 'PATCH')).toBe(false);
    const select = screen.getByRole('combobox', { name: 'Lanes by' });
    await vi.waitFor(() => {
      const labels = [...select.element().querySelectorAll('option')].map((one) => one.text);
      expect(labels).toEqual(['Choose…', 'Priority', 'Project', 'Tags', 'Created', 'Updated']);
    });
    await select.selectOptions('Priority');
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({
        lanes: {
          mode: 'values',
          field: PRIORITY.id,
          attribute: null,
          values: [],
          other: 'bottom',
          new_card_value: null,
        },
        version: 3,
      });
    expect(onchanged).toHaveBeenCalled();
    await expectAccessible(screen.container.ownerDocument.body);
  });

  it('adds a lane, places «Other», and turns lanes off', async () => {
    const calls = settingsServer({});
    const shown = board({
      lanes: { ...BY_PRIORITY, values: ['v:71'], labels: BY_PRIORITY.labels.slice(0, 1) },
    });
    const { screen } = await lanesTab(shown);
    const list = screen.getByRole('list', { name: 'Lanes in order' });
    await expect.element(list.getByText('Major')).toBeVisible();
    await screen.getByRole('checkbox', { name: 'Minor' }).click();
    await expect
      .poll(() => calls.filter((call) => call.method === 'PATCH').at(-1)?.body)
      .toMatchObject({ lanes: { values: ['v:71', 'v:72'] }, version: 3 });

    await screen.getByRole('radio', { name: 'At the top' }).click();
    await expect
      .poll(() => calls.filter((call) => call.method === 'PATCH').at(-1)?.body)
      .toMatchObject({ lanes: { other: 'top', values: ['v:71'] } });

    await screen.getByRole('radio', { name: /No lanes/ }).click();
    await expect
      .poll(() => calls.filter((call) => call.method === 'PATCH').at(-1)?.body)
      .toEqual({
        lanes: {
          mode: 'none',
          field: null,
          attribute: null,
          values: [],
          other: 'bottom',
          new_card_value: null,
        },
        version: 3,
      });
  });

  it('gives parent lanes the value new cards get (BRD-055)', async () => {
    const calls = settingsServer({});
    const shown = board({
      lanes: { ...BY_PRIORITY, mode: 'parents', values: [], labels: [] },
    });
    const { screen } = await lanesTab(shown);
    const select = screen.getByRole('combobox', { name: 'New cards get' });
    await vi.waitFor(() => {
      expect(select.element().querySelectorAll('option')).toHaveLength(3);
    });
    await select.selectOptions('Minor');
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toMatchObject({ lanes: { mode: 'parents', field: PRIORITY.id, new_card_value: 72 } });
  });
});

describe.skipIf(!isPhone())('BRD-088: lanes on a phone', () => {
  it('shows the lanes with one column at a time and collapses a lane', async () => {
    const calls = server({
      'PATCH /boards/9/profile': (request) => ({
        body: { ...view().profile, ...(request.body as object) },
      }),
    });
    const screen = await page();
    await expect.element(screen.getByRole('navigation', { name: 'Board columns' })).toBeVisible();
    expect(summaries('Open', 'Major')).toEqual(['Login form']);
    await screen.getByRole('button', { name: 'Collapse lane “Major”' }).click();
    await expect
      .poll(() => calls.find((call) => call.method === 'PATCH')?.body)
      .toEqual({ collapsed_lanes: ['v:71'] });
    await expectAccessible(screen.container);
  });
});
