// The live board on a desktop and on a phone (step 1.8a-6; BRD-114, BRD-115): the page follows
// `board:9` through a fake connection the test drives by hand.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { CardBatchAnswer, LiveEvent } from '#lib/api/schema.gen.ts';
import type { TopicHandlers } from '#lib/live/connection.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';

import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { MEMBER, SESSION, board, issue, view } from '../test-data.ts';
import BoardPage from './BoardPage.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

const TOPIC = 'board:9';

/** A connection whose topics the test drives by hand. */
class FakeConnection {
  readonly handlers = new Map<string, TopicHandlers>();

  subscribe(topic: string, handlers: TopicHandlers): () => void {
    this.handlers.set(topic, handlers);
    return () => {
      this.handlers.delete(topic);
    };
  }

  get board(): TopicHandlers {
    const handlers = this.handlers.get(TOPIC);
    if (!handlers) throw new Error(`not subscribed to ${TOPIC}`);
    return handlers;
  }

  send(
    kind: string,
    id: number,
    change: LiveEvent['change'] = 'updated',
    patch: LiveEvent['patch'] = null,
  ): void {
    this.board.event?.({
      id: 1,
      topics: [TOPIC],
      entity: { kind, id, version: 50 },
      change,
      patch,
    });
  }
}

function server(routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({ 'GET /boards/9/view': { body: view() }, ...routes });
}

async function shown(connection: FakeConnection) {
  const screen = await render(BoardPage, { board: board(), live: connection });
  await expect.element(screen.getByRole('region', { name: 'Columns' })).toBeVisible();
  await vi.waitFor(() => {
    expect(connection.handlers.has(TOPIC)).toBe(true);
  });
  return screen;
}

function summaries(name: string): string[] {
  const list = document.querySelector(`[aria-label="Cards of “${name}”"]`);
  return [...(list?.querySelectorAll('article .summary') ?? [])].map((one) => one.textContent);
}

const NO_REFS = { fields: [], values: [], users: [], groups: [] };

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-114: the live board', () => {
  it('asks where changed cards stand, in one batch, and places them', async () => {
    const connection = new FakeConnection();
    const calls = server({
      'POST /boards/9/cards/batch': (request): { body: CardBatchAnswer } => {
        const { issues } = request.body as { issues: number[] };
        expect(issues).toEqual([102, 104, 105]);
        return {
          body: {
            cards: [
              { issue: 102, column: 3, lane: null, rank: 'b' },
              null,
              { issue: 105, column: 1, lane: null, rank: '0' },
            ],
            issues: [issue(102, 'Logout, renamed', { version: 2 }), issue(105, 'New one')],
            refs: NO_REFS,
          },
        };
      },
    });
    await shown(connection);

    connection.send('issue', 102);
    connection.send('issue', 104);
    connection.send('issue', 105, 'created');
    await vi.waitFor(() => {
      expect(summaries('Done')).toEqual(['Logout, renamed']);
    });
    expect(summaries('Open')).toEqual(['New one', 'Login form']);
    const batches = calls.filter((call) => call.path === '/boards/9/cards/batch');
    expect(batches).toHaveLength(1);
    expect(batches[0]?.body).toEqual({ issues: [102, 104, 105], sprint: 7 });
  });

  it('drops a deleted issue at once', async () => {
    const connection = new FakeConnection();
    const calls = server({});
    await shown(connection);
    connection.send('issue', 101, 'deleted');
    await vi.waitFor(() => {
      expect(summaries('Open')).toEqual(['Logout']);
    });
    expect(calls.some((call) => call.path.endsWith('/batch'))).toBe(false);
  });

  it('reloads the view when the board changes, and shows not found when it is gone', async () => {
    const connection = new FakeConnection();
    const calls = server({});
    const screen = await shown(connection);
    connection.send('board', 9);
    await vi.waitFor(() => {
      expect(calls.filter((call) => call.path === '/boards/9/view')).toHaveLength(2);
    });
    connection.send('board', 9, 'deleted');
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Nothing here' }))
      .toBeVisible();
    expect(screen.container.querySelector('[aria-label="Columns"]')).toBeNull();
  });

  it('shows not found when the board is no longer shared with the person', async () => {
    const connection = new FakeConnection();
    server({});
    const screen = await shown(connection);
    connection.board.denied?.();
    await vi.waitFor(() => {
      expect(screen.container.querySelector('[aria-label="Columns"]')).toBeNull();
    });
  });

  it('reloads after missed changes that cannot be replayed', async () => {
    const connection = new FakeConnection();
    const calls = server({});
    await shown(connection);
    connection.board.resync?.();
    await vi.waitFor(() => {
      expect(calls.filter((call) => call.path === '/boards/9/view')).toHaveLength(2);
    });
  });
});

describe('BRD-115: a reorder from someone else', () => {
  it('moves the card after its new neighbour without a request', async () => {
    const connection = new FakeConnection();
    const calls = server({});
    await shown(connection);
    connection.send('card_order', 101, 'updated', { issue: 101, after: 102, sprint: 7 });
    await vi.waitFor(() => {
      expect(summaries('Open')).toEqual(['Logout', 'Login form']);
    });
    connection.send('card_order', 101, 'updated', { issue: 101, after: null, sprint: 7 });
    await vi.waitFor(() => {
      expect(summaries('Open')).toEqual(['Login form', 'Logout']);
    });
    // Another sprint's order does not touch this one.
    connection.send('card_order', 101, 'updated', { issue: 101, after: 102, sprint: 8 });
    await new Promise((done) => setTimeout(done, 150));
    expect(summaries('Open')).toEqual(['Login form', 'Logout']);
    expect(calls.some((call) => call.path.endsWith('/batch'))).toBe(false);
  });

  it('asks the board when the neighbour is not shown', async () => {
    const connection = new FakeConnection();
    const calls = server({
      'POST /boards/9/cards/batch': {
        body: {
          cards: [{ issue: 101, column: 2, lane: null, rank: 'z' }],
          issues: [],
          refs: NO_REFS,
        },
      },
    });
    await shown(connection);
    connection.send('card_order', 101, 'updated', { issue: 101, after: 999, sprint: 7 });
    await vi.waitFor(() => {
      expect(calls.some((call) => call.path.endsWith('/batch'))).toBe(true);
    });
    // It ranks after the shown cards of a truncated cell: only the count changes.
    await vi.waitFor(() => {
      expect(summaries('Open')).toEqual(['Logout']);
    });
    expect(summaries('In Progress')).toEqual(['Search']);
  });
});
