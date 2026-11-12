// The board on a phone (step 1.8a-7; BRD-088, D-044): one column at a time with the strip
// of columns, the overview of all columns, the zoom remembered per board, a card held at the edge
// turning to the next column. On a desktop there is no zoom.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { PlacedCard } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { LONG_PRESS_MS, PAGE_HOLD_MS } from '../drag/drag.svelte.ts';
import { MEMBER, SESSION, board, issue, view } from '../test-data.ts';
import BoardPage from './BoardPage.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

const ZOOM_KEY = 'trackflow.board-zoom.9';

function server(routes: Record<string, FakeRoute> = {}): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({ 'GET /boards/9/view': { body: view() }, ...routes });
}

async function shown() {
  const screen = await render(BoardPage, {
    board: board(),
    live: { subscribe: () => () => undefined },
  });
  await expect.element(screen.getByRole('region', { name: 'Columns' })).toBeVisible();
  return screen;
}

function grid(): HTMLElement {
  const found = document.querySelector<HTMLElement>('[role="region"][aria-label="Columns"]');
  if (!found) throw new Error('no grid');
  return found;
}

/** The column whose left side is at the grid's left side. */
function columnAtStart(): string | null {
  const left = grid().getBoundingClientRect().left;
  const columns = [...grid().querySelectorAll<HTMLElement>(':scope > section')];
  const at = columns.find(
    (one) => Math.abs(one.getBoundingClientRect().left - left) < one.clientWidth / 2,
  );
  return at?.getAttribute('aria-label') ?? null;
}

const pause = (ms: number) => new Promise((done) => setTimeout(done, ms));

afterEach(() => {
  toasts.clear();
  localStorage.removeItem(ZOOM_KEY);
  setSessionForTests(null);
  setAccessForTests(null);
});

describe.skipIf(!isPhone())('BRD-088: the board on a phone', () => {
  it('shows one column at a time; the strip names the columns and turns to one', async () => {
    server();
    const screen = await shown();

    await expect
      .element(screen.getByRole('button', { name: 'Column', exact: true }))
      .toHaveAttribute('aria-pressed', 'true');
    const strip = screen.getByRole('navigation', { name: 'Board columns' });
    await expect
      .element(strip.getByRole('button', { name: /^Open/ }))
      .toHaveAttribute('aria-current', 'true');
    expect(columnAtStart()).toBe('Open');
    // A column fills the screen; its cards keep the chosen size.
    const open = grid().querySelector<HTMLElement>('section[aria-label="Open"]');
    expect(open?.clientWidth).toBeGreaterThan(window.innerWidth * 0.8);

    await strip.getByRole('button', { name: /^Done/ }).click();
    await vi.waitFor(() => {
      expect(columnAtStart()).toBe('Done');
    });
    await expect
      .element(strip.getByRole('button', { name: /^Done/ }))
      .toHaveAttribute('aria-current', 'true');
    // Collapsing has no place here: the column is the whole screen.
    expect(screen.getByRole('button', { name: 'Collapse “Open”' }).elements()).toHaveLength(0);
    await expectAccessible(screen.container);
  });

  it('shows all columns narrow with small cards; a header hides its cards; it is remembered', async () => {
    server();
    const screen = await shown();

    await screen.getByRole('button', { name: 'All', exact: true }).click();
    await expect
      .element(screen.getByRole('button', { name: 'All', exact: true }))
      .toHaveAttribute('aria-pressed', 'true');
    expect(localStorage.getItem(ZOOM_KEY)).toBe('all');
    expect(screen.getByRole('navigation', { name: 'Board columns' }).elements()).toHaveLength(0);
    expect(screen.getByRole('button', { name: /^Card size/ }).elements()).toHaveLength(0);
    expect(document.querySelector('[data-card] article.size-0')).not.toBeNull();
    // Every column is on the screen at once.
    for (const name of ['Open', 'In Progress']) {
      const column = grid().querySelector<HTMLElement>(`section[aria-label="${name}"]`);
      expect(column?.getBoundingClientRect().right).toBeLessThanOrEqual(window.innerWidth);
    }

    const header = screen.getByRole('button', { name: /^Open/, expanded: true });
    await header.click();
    await expect
      .element(screen.getByRole('button', { name: /^Open/, expanded: false }))
      .toBeVisible();
    expect(screen.getByRole('list', { name: 'Cards of “Open”' }).elements()).toHaveLength(0);
    await expectAccessible(screen.container);

    await screen.unmount();
    const again = await shown();
    await expect
      .element(again.getByRole('button', { name: 'All', exact: true }))
      .toHaveAttribute('aria-pressed', 'true');
  });

  it('turns to the next column when a dragged card is held at the edge', async () => {
    const calls = server({
      'POST /boards/9/sprints/7/cards/102/move': (request): { body: PlacedCard } => {
        const body = request.body as { column: number };
        const moved = issue(102, 'Logout');
        return {
          body: {
            card: { issue: 102, column: body.column, lane: null, rank: '0' },
            issue: { ...moved, version: moved.version + 1 },
            refs: { fields: [], values: [], users: [], groups: [] },
            changes: [],
            messages: [],
          },
        };
      },
    });
    await shown();
    const card = document.querySelector<HTMLElement>('[data-card="102"] .summary');
    if (!card) throw new Error('no card');
    const rect = card.getBoundingClientRect();
    const send = (type: string, x: number, y: number) =>
      card.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          clientX: x,
          clientY: y,
          pointerId: 1,
          pointerType: 'touch',
          isPrimary: true,
          button: 0,
        }),
      );
    send('pointerdown', rect.left + 10, rect.top + 5);
    await pause(LONG_PRESS_MS + 50);
    const edge = { x: window.innerWidth - 10, y: rect.top + 5 };
    send('pointermove', rect.left + 40, rect.top + 5);
    send('pointermove', edge.x, edge.y);
    await pause(PAGE_HOLD_MS + 300);
    await vi.waitFor(() => {
      expect(columnAtStart()).toBe('In Progress');
    });
    // Back from the edge, then dropped on the column now shown.
    const middle = { x: window.innerWidth / 2, y: edge.y };
    send('pointermove', middle.x, middle.y);
    await pause(50);
    send('pointerup', middle.x, middle.y);

    await vi.waitFor(() => {
      expect(calls.find((call) => call.method === 'POST')?.body).toMatchObject({ column: 2 });
    });
  });
});

describe.skipIf(isPhone())('BRD-088: the board on a desktop', () => {
  it('has no zoom and no strip of columns', async () => {
    server();
    const screen = await shown();

    await expect.element(screen.getByRole('button', { name: /^Card size/ })).toBeVisible();
    expect(screen.getByRole('group', { name: 'Show' }).elements()).toHaveLength(0);
    expect(screen.getByRole('navigation', { name: 'Board columns' }).elements()).toHaveLength(0);
  });
});
