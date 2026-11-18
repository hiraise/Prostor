// The backlog panel and adding from it (step 1.8b-6; BRD-028, BRD-074, BRD-089, BRD-093,
// BRD-096, BRD-116): the toggle, the rows with the header filter, «В спринт», the query-board
// preview, dragging rows and cards between the panel and the cells, the live reorder.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { goto } from '$app/navigation';

import type { BoardBacklog, CardIssue, Issue, LiveEvent, PlacedCard } from '#lib/api/schema.gen.ts';
import { issue as fullIssue } from '#lib/features/issues/test-data.ts';
import { newIssueDialog } from '#lib/features/issues/new/open.svelte.ts';
import type { TopicHandlers } from '#lib/live/connection.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { writeStored } from '#lib/storage.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import BoardPage from '../board/BoardPage.svelte';
import { headerFilter } from '../board/header-filter.svelte.ts';
import { ASSIGNEE, BORIS, MEMBER, SESSION, board, issue, view } from '../test-data.ts';
import { changeLines } from './query-mode.ts';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

const ROWS = [
  fullIssue({ id: 201, readable_id: 'WEB-21', summary: 'Export to CSV' }),
  fullIssue({ id: 202, readable_id: 'WEB-22', summary: 'Dark theme' }),
  fullIssue({ id: 203, readable_id: 'WEB-23', summary: 'Keyboard help' }),
];
const NO_REFS = { fields: [], values: [], users: [], groups: [] };
const ADD = 'POST /boards/9/sprints/7/cards';

function backlog(items: Issue[] = ROWS, extra: Partial<BoardBacklog> = {}): BoardBacklog {
  return {
    search: { id: 5, name: 'Team · backlog', may_order: true },
    missing: null,
    page: {
      items,
      next_cursor: null,
      sort: [],
      notes: [],
      query: '',
      single_id: null,
      live_topic: 'issues:backlog',
    },
    ...extra,
  };
}

/** The server's answer to an add: a card in column 1 for the issue. */
function placed(id: number, summary: string, extra: Partial<PlacedCard> = {}): PlacedCard {
  const shown: CardIssue = issue(id, summary);
  return {
    card: { issue: id, column: 1, lane: null, rank: 'c' },
    issue: shown,
    refs: NO_REFS,
    changes: [],
    messages: [],
    ...extra,
  };
}

/** A connection whose topics the test drives by hand. */
class FakeConnection {
  readonly handlers = new Map<string, TopicHandlers>();

  subscribe(topic: string, handlers: TopicHandlers): () => void {
    this.handlers.set(topic, handlers);
    return () => {
      this.handlers.delete(topic);
    };
  }

  send(topic: string, kind: string, id: number, patch: LiveEvent['patch'] = null): void {
    this.handlers.get(topic)?.event?.({
      id: 1,
      topics: [topic],
      entity: { kind, id, version: 50 },
      change: 'updated',
      patch,
    });
  }
}

function server(routes: Record<string, FakeRoute> = {}, shownBoard = board()): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({
    'GET /boards/9/view': { body: view({ board: shownBoard }) },
    'GET /boards/9/backlog': { body: backlog() },
    ...routes,
  });
}

async function shown(shownBoard = board(), live = new FakeConnection()) {
  const screen = await render(BoardPage, { board: shownBoard, live });
  await expect.element(screen.getByRole('region', { name: 'Columns' })).toBeVisible();
  return screen;
}

async function openPanel(screen: Awaited<ReturnType<typeof shown>>) {
  await screen.getByRole('button', { name: 'Backlog', exact: true }).click();
  const rows = screen.getByRole('list', { name: 'Backlog issues' });
  await expect.element(rows).toBeVisible();
  await vi.waitFor(() => {
    expect(rowIds()).toEqual([201, 202, 203]);
  });
  return rows;
}

function rowIds(): number[] {
  return [...document.querySelectorAll<HTMLElement>('[data-drop-backlog] [data-card]')].map((one) =>
    Number(one.dataset.card),
  );
}

function summaries(name: string): string[] {
  const list = document.querySelector(`[aria-label="Cards of “${name}”"]`);
  return [...(list?.querySelectorAll('article .summary') ?? [])].map((one) => one.textContent);
}

beforeEach(() => {
  headerFilter.clear();
  toasts.clear();
  writeStored('prostor.board-backlog.9', null);
});

afterEach(() => {
  // The panel stays open per board in this browser: other test files expect it closed.
  writeStored('prostor.board-backlog.9', null);
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-089, BRD-091: the backlog panel', () => {
  it('opens from the toolbar with the header filter applied and is accessible', async () => {
    headerFilter.set('#Bug');
    const calls = server();
    const screen = await shown();
    const toggle = screen.getByRole('button', { name: 'Backlog', exact: true });
    await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');
    await openPanel(screen);
    await expect.element(toggle).toHaveAttribute('aria-pressed', 'true');
    const asked = calls.filter((call) => call.path === '/boards/9/backlog');
    expect(asked.at(-1)?.query.get('q')).toBe('#Bug');
    await expect.element(screen.getByText('Team · backlog')).toBeVisible();
    await expect
      .element(screen.getByRole('link', { name: 'Open as a list' }))
      .toHaveAttribute('href', '/issues?search=5');
    await expectAccessible(document.body);
  });

  it('says why there is none', async () => {
    server({
      'GET /boards/9/backlog': { body: backlog([], { search: null, missing: 'none', page: null }) },
    });
    const screen = await shown();
    await screen.getByRole('button', { name: 'Backlog', exact: true }).click();
    await expect.element(screen.getByText(/The board has no backlog yet/)).toBeVisible();
  });

  it('says when its search is not shared with the person', async () => {
    server({
      'GET /boards/9/backlog': {
        body: backlog([], { search: null, missing: 'hidden', page: null }),
      },
    });
    const screen = await shown();
    await screen.getByRole('button', { name: 'Backlog', exact: true }).click();
    await expect.element(screen.getByText(/not shared with you/)).toBeVisible();
  });
});

describe('BRD-094: a new issue from the backlog', () => {
  it('opens the new-issue form in the first project of the board', async () => {
    server();
    const screen = await shown();
    await openPanel(screen);
    await screen.getByRole('button', { name: 'New issue', exact: true }).click();
    const key = board().projects[0]?.project.key;
    if (isPhone()) {
      expect(vi.mocked(goto)).toHaveBeenCalled();
    } else {
      expect(newIssueDialog.target).toEqual({ project: key });
      newIssueDialog.open = false;
    }
  });
});

describe('BRD-096: from the backlog to the board', () => {
  it('adds a row with its button; the row leaves the panel', async () => {
    const calls = server({ [ADD]: { body: placed(201, 'Export to CSV') } });
    const screen = await shown();
    await openPanel(screen);
    await screen.getByRole('button', { name: 'Add WEB-21 to the board' }).click();
    await vi.waitFor(() => {
      expect(rowIds()).toEqual([202, 203]);
    });
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({ issue: 201 });
    expect(summaries('Open')).toEqual(['Login form', 'Logout', 'Export to CSV']);
    expect(toasts.items.at(-1)?.message).toBe('Added to the board');
  });

  it('keeps the row and says why when the server refuses', async () => {
    server({ [ADD]: failure(403, 'forbidden') });
    const screen = await shown();
    await openPanel(screen);
    await screen.getByRole('button', { name: 'Add WEB-21 to the board' }).click();
    await vi.waitFor(() => {
      expect(toasts.items.at(-1)?.message).toBe('Could not add the issue');
    });
    expect(rowIds()).toEqual([201, 202, 203]);
  });
});

describe('BRD-028: adding to a query board shows the changes first', () => {
  const QUERY = board({ mode: 'query', query: 'Assignee: boris' });
  const preview = placed(201, 'Export to CSV', {
    changes: [{ kind: 'set', field: ASSIGNEE.id, value: BORIS.id }],
    issue: issue(201, 'Export to CSV', {
      fields: [{ field: ASSIGNEE.id, ids: [BORIS.id], text: null, number: null }],
    }),
    refs: { fields: [ASSIGNEE], values: [], users: [BORIS], groups: [] },
  });

  it('lists «field: was → becomes» and adds only after «Add»', async () => {
    const bodies: unknown[] = [];
    server(
      {
        [ADD]: (request) => {
          bodies.push(request.body);
          return { body: preview };
        },
      },
      QUERY,
    );
    const screen = await shown(QUERY);
    await openPanel(screen);
    await screen.getByRole('button', { name: 'Add WEB-21 to the board' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Add WEB-21 to the board?' });
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog.getByText('Assignee: empty → Boris Lead')).toBeInTheDocument();
    expect(bodies).toEqual([{ issue: 201, preview: true }]);
    expect(rowIds()).toEqual([201, 202, 203]);
    await dialog.getByRole('button', { name: 'Add' }).click();
    await vi.waitFor(() => {
      expect(rowIds()).toEqual([202, 203]);
    });
    expect(bodies).toEqual([{ issue: 201, preview: true }, { issue: 201 }]);
  });

  it('shows the reason when no change can make it match', async () => {
    server(
      {
        [ADD]: {
          status: 403,
          body: { code: 'forbidden', message: 'No field change makes the issue match' },
        },
      },
      QUERY,
    );
    const screen = await shown(QUERY);
    await openPanel(screen);
    await screen.getByRole('button', { name: 'Add WEB-21 to the board' }).click();
    await vi.waitFor(() => {
      expect(toasts.items.at(-1)).toMatchObject({
        message: 'This issue cannot join the board',
        detail: 'No field change makes the issue match',
      });
    });
    await expect
      .element(screen.getByRole('dialog', { name: 'Add WEB-21 to the board?' }))
      .not.toBeInTheDocument();
  });

  it('names the values before and after', () => {
    const lines = changeLines(
      {
        changes: [
          { kind: 'set', field: ASSIGNEE.id, value: BORIS.id },
          { kind: 'clear', field: 11 },
          { kind: 'tag', tag: 4 },
        ],
        issue: issue(201, 'x', {
          fields: [{ field: ASSIGNEE.id, ids: [BORIS.id], text: null, number: null }],
          tags: [{ id: 4, name: 'later', color: null }],
        }),
        refs: {
          fields: [ASSIGNEE, { ...ASSIGNEE, id: 11, name: 'Priority', kind: 'enum' }],
          values: [],
          users: [BORIS],
          groups: [],
        },
      },
      fullIssue(),
    );
    expect(lines).toEqual([
      { field: 'Assignee', was: 'empty', now: 'Boris Lead' },
      { field: 'Priority', was: 'Major', now: 'empty' },
      { field: 'Tags', was: 'empty', now: 'later' },
    ]);
  });
});

const frame = () =>
  new Promise<void>((done) => {
    requestAnimationFrame(() => {
      done();
    });
  });

function pointer(type: string, target: EventTarget, x: number, y: number): void {
  target.dispatchEvent(
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
}

/** Drags an element with a mouse to a point and drops it there. */
async function drag(element: HTMLElement, to: { x: number; y: number }): Promise<void> {
  const start = element.getBoundingClientRect();
  const from = { x: start.left + 10, y: start.top + 5 };
  pointer('pointerdown', element, from.x, from.y);
  for (const step of [0.1, 0.5, 1]) {
    pointer(
      'pointermove',
      element,
      from.x + (to.x - from.x) * step,
      from.y + (to.y - from.y) * step,
    );
    await frame();
  }
  await frame();
  pointer('pointerup', element, to.x, to.y);
}

function element(selector: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(selector);
  if (!found) throw new Error(`nothing at ${selector}`);
  return found;
}

function above(selector: string): { x: number; y: number } {
  const rect = element(selector).getBoundingClientRect();
  return { x: rect.left + rect.width / 2, y: rect.top + 2 };
}

// Dragging needs the panel beside the columns: phones use the buttons.
describe.skipIf(isPhone())('BRD-093: dragging in and out of the backlog', () => {
  it('reorders rows and keeps the order on the server; «Reset the order» forgets it', async () => {
    const calls = server({
      'POST /boards/9/backlog/order': { status: 204 },
      'DELETE /boards/9/backlog/order': { status: 204 },
    });
    const screen = await shown();
    await openPanel(screen);
    await drag(
      element('[data-drop-backlog] [data-card="203"]'),
      above('[data-drop-backlog] [data-card="201"]'),
    );
    await vi.waitFor(() => {
      expect(rowIds()).toEqual([203, 201, 202]);
    });
    const order = calls.find((call) => call.path === '/boards/9/backlog/order');
    expect(order?.body).toEqual({ issue: 203, before: 201 });

    await screen.getByRole('button', { name: 'Backlog actions' }).click();
    await screen.getByRole('menuitem', { name: 'Reset the order' }).click();
    await vi.waitFor(() => {
      expect(calls.some((call) => call.method === 'DELETE')).toBe(true);
    });
    await vi.waitFor(() => {
      expect(rowIds()).toEqual([201, 202, 203]);
    });
  });

  it('puts a row onto a cell between its cards (BRD-096)', async () => {
    const calls = server({
      [ADD]: {
        body: placed(202, 'Dark theme', { card: { issue: 202, column: 2, lane: null, rank: '0' } }),
      },
    });
    const screen = await shown();
    await openPanel(screen);
    await drag(
      element('[data-drop-backlog] [data-card="202"]'),
      above('[data-drop-column="2"] [data-card="103"]'),
    );
    await vi.waitFor(() => {
      expect(rowIds()).toEqual([201, 203]);
    });
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({
      issue: 202,
      column: 2,
      before: 103,
    });
    expect(summaries('In Progress')[0]).toBe('Dark theme');
  });

  it('takes a card dropped on the panel off the board (BRD-074)', async () => {
    const calls = server({ 'DELETE /boards/9/sprints/7/cards/102': { status: 204 } });
    const screen = await shown();
    await openPanel(screen);
    await drag(
      element('[data-card="102"] .summary'),
      above('[data-drop-backlog] [data-card="202"]'),
    );
    await vi.waitFor(() => {
      expect(summaries('Open')).toEqual(['Login form']);
    });
    expect(calls.some((call) => call.method === 'DELETE' && call.path.endsWith('/cards/102'))).toBe(
      true,
    );
  });
});

describe('BRD-116: the backlog live', () => {
  it('moves a row in place when another person reorders the backlog', async () => {
    server();
    const live = new FakeConnection();
    const screen = await shown(board(), live);
    await openPanel(screen);
    await vi.waitFor(() => {
      expect(live.handlers.has('board:9')).toBe(true);
    });
    live.send('board:9', 'card_order', 5, { issue: 203, after: 201, search: 5 });
    await vi.waitFor(() => {
      expect(rowIds()).toEqual([201, 203, 202]);
    });
  });

  it('offers «Refresh» when an issue it does not show changes', async () => {
    server();
    const live = new FakeConnection();
    const screen = await shown(board(), live);
    await openPanel(screen);
    await vi.waitFor(() => {
      expect(live.handlers.has('issues:backlog')).toBe(true);
    });
    live.send('issues:backlog', 'issue', 299);
    await expect.element(screen.getByText(/The backlog has changed/)).toBeVisible();
  });
});
