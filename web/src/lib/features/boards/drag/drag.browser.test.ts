// Dragging cards on a desktop and on a phone (step 1.8a-6; BRD-068, BRD-069): a mouse drag, a touch
// long press, Esc, a refused move that snaps back, the undo toast, and the card menu.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { CardIssue, PlacedCard } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import BoardPage from '../board/BoardPage.svelte';
import { MEMBER, SESSION, board, issue, view } from '../test-data.ts';
import { LONG_PRESS_MS } from './drag.svelte.ts';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

const MOVE = 'POST /boards/9/sprints/7/cards/102/move';

/** The server's answer to a move: the card where it asked to be. */
function placed(column: number, rank: string, shown: CardIssue = issue(102, 'Logout')): PlacedCard {
  return {
    card: { issue: shown.id, column, lane: null, rank },
    issue: { ...shown, version: shown.version + 1 },
    refs: { fields: [], values: [], users: [], groups: [] },
    changes: [],
    messages: [],
  };
}

function server(routes: Record<string, FakeRoute>): FakeRequest[] {
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

function summaries(name: string): string[] {
  const list = document.querySelector(`[aria-label="Cards of “${name}”"]`);
  return [...(list?.querySelectorAll('article .summary') ?? [])].map((one) => one.textContent);
}

function cardElement(id: number): HTMLElement {
  const card = document.querySelector<HTMLElement>(`[data-card="${String(id)}"] .summary`);
  if (!card) throw new Error(`no card ${String(id)}`);
  return card;
}

/** A point at the top of a cell, above its first card's middle. */
function cellTop(column: number): { x: number; y: number } {
  const cell = document.querySelector<HTMLElement>(`[data-drop-column="${String(column)}"]`);
  if (!cell) throw new Error(`no cell ${String(column)}`);
  cell.scrollIntoView({ block: 'nearest', inline: 'center' });
  const rect = cell.getBoundingClientRect();
  return { x: rect.left + rect.width / 2, y: rect.top + 6 };
}

function pointer(
  type: string,
  target: EventTarget,
  x: number,
  y: number,
  pointerType = 'mouse',
): void {
  target.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      clientX: x,
      clientY: y,
      pointerId: 1,
      pointerType,
      isPrimary: true,
      button: 0,
    }),
  );
}

const frame = () =>
  new Promise<void>((done) => {
    requestAnimationFrame(() => {
      done();
    });
  });

/** Presses a card, moves to a point in steps, and waits for the frames that follow. */
async function dragTo(
  id: number,
  to: { x: number; y: number },
  pointerType = 'mouse',
): Promise<void> {
  const card = cardElement(id);
  const start = card.getBoundingClientRect();
  const from = { x: start.left + 10, y: start.top + 5 };
  pointer('pointerdown', card, from.x, from.y, pointerType);
  if (pointerType !== 'mouse') await new Promise((done) => setTimeout(done, LONG_PRESS_MS + 50));
  for (const step of [0.1, 0.5, 1]) {
    pointer(
      'pointermove',
      card,
      from.x + (to.x - from.x) * step,
      from.y + (to.y - from.y) * step,
      pointerType,
    );
    await frame();
  }
  await frame();
}

afterEach(() => {
  toasts.clear();
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-068: dragging a card', () => {
  it('moves it to another column with a mouse and offers to undo', async () => {
    const calls = server({
      [MOVE]: (request) => {
        const body = request.body as { column: number };
        return { body: placed(body.column, body.column === 2 ? '0' : 'b') };
      },
    });
    await shown();
    const to = cellTop(2);
    await dragTo(102, to);
    // The copy follows the pointer and a line shows where the card lands.
    expect(document.querySelector('[data-drag-ghost]')).not.toBeNull();
    expect(document.querySelector<HTMLElement>('[data-drop-line]')?.hidden).toBe(false);
    pointer('pointerup', cardElement(102), to.x, to.y);

    expect(document.querySelector('[data-drag-ghost]')).toBeNull();
    await vi.waitFor(() => {
      expect(summaries('In Progress')).toEqual(['Logout', 'Search']);
    });
    expect(summaries('Open')).toEqual(['Login form']);
    await vi.waitFor(() => {
      expect(calls.filter((call) => call.path.endsWith('/move'))).toHaveLength(1);
    });
    expect(calls.find((call) => call.path.endsWith('/move'))?.body).toEqual({
      column: 2,
      version: 1,
      before: 103,
    });
    await vi.waitFor(() => {
      expect(toasts.items[0]?.message).toBe('Moved');
    });

    // Undo puts it back between its old neighbours.
    const toast = toasts.items[0];
    if (toast) await toasts.undo(toast.id);
    await vi.waitFor(() => {
      expect(summaries('Open')).toEqual(['Login form', 'Logout']);
    });
    expect(calls.filter((call) => call.path.endsWith('/move')).at(-1)?.body).toEqual({
      column: 1,
      version: 2,
      after: 101,
    });
  });

  it('starts with a long press on a touch screen, not with a swipe', async () => {
    const calls = server({ [MOVE]: { body: placed(3, '0') } });
    await shown();

    // A swipe scrolls: the press ends before it lasts long enough.
    const card = cardElement(102);
    const rect = card.getBoundingClientRect();
    pointer('pointerdown', card, rect.left + 5, rect.top + 5, 'touch');
    pointer('pointermove', card, rect.left + 5, rect.top + 40, 'touch');
    await new Promise((done) => setTimeout(done, LONG_PRESS_MS + 50));
    expect(document.querySelector('[data-drag-ghost]')).toBeNull();
    pointer('pointerup', card, rect.left + 5, rect.top + 40, 'touch');

    const to = cellTop(3);
    await dragTo(102, to, 'touch');
    expect(document.querySelector('[data-drag-ghost]')).not.toBeNull();
    pointer('pointerup', cardElement(102), to.x, to.y, 'touch');
    await vi.waitFor(() => {
      expect(summaries('Done')).toEqual(['Logout', 'Done already']);
    });
    expect(calls.filter((call) => call.path.endsWith('/move'))).toHaveLength(1);
  });

  it('cancels with Esc and leaves the card in its place', async () => {
    const calls = server({ [MOVE]: { body: placed(2, '0') } });
    await shown();
    const to = cellTop(2);
    await dragTo(102, to);
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    pointer('pointerup', cardElement(102), to.x, to.y);
    expect(document.querySelector('[data-drag-ghost]')).toBeNull();
    expect(summaries('Open')).toEqual(['Login form', 'Logout']);
    await new Promise((done) => setTimeout(done, 50));
    expect(calls.filter((call) => call.path.endsWith('/move'))).toHaveLength(0);
  });
});

describe('BRD-069: a refused move', () => {
  it('snaps the card back and says why', async () => {
    server({
      [MOVE]: {
        status: 403,
        body: { code: 'forbidden', message: 'State: a reviewer is required' },
      },
    });
    await shown();
    const to = cellTop(2);
    await dragTo(102, to);
    pointer('pointerup', cardElement(102), to.x, to.y);
    await vi.waitFor(() => {
      expect(toasts.items[0]?.message).toBe('Could not move the card');
    });
    expect(toasts.items[0]?.detail).toBe('State: a reviewer is required');
    expect(summaries('Open')).toEqual(['Login form', 'Logout']);
    expect(summaries('In Progress')).toEqual(['Search']);
  });
});

describe('BRD-068: the card menu', () => {
  it('moves a card without a pointer and takes it off the board', async () => {
    const calls = server({
      [MOVE]: { body: placed(1, '0') },
      'DELETE /boards/9/sprints/7/cards/102': { status: 204 },
    });
    const screen = await shown();

    await screen.getByRole('button', { name: 'Actions of WEB-102' }).click();
    await screen.getByRole('menuitem', { name: 'To the top' }).click();
    await vi.waitFor(() => {
      expect(summaries('Open')).toEqual(['Logout', 'Login form']);
    });
    expect(calls.find((call) => call.path.endsWith('/move'))?.body).toEqual({
      column: 1,
      version: 1,
      before: 101,
    });

    await screen.getByRole('button', { name: 'Actions of WEB-102' }).click();
    await screen.getByRole('menuitem', { name: 'Remove from board' }).click();
    await vi.waitFor(() => {
      expect(summaries('Open')).toEqual(['Login form']);
    });
    await vi.waitFor(() => {
      expect(toasts.items.some((one) => one.message === 'Removed from the board')).toBe(true);
    });
    expect(calls.some((call) => call.method === 'DELETE')).toBe(true);
  });

  it('offers only «Open» on an issue the person may not change', async () => {
    const readOnly = view({
      issues: view().issues.map((one) => ({ ...one, can_update: false })),
    });
    server({ 'GET /boards/9/view': { body: readOnly } });
    const screen = await shown();
    await screen.getByRole('button', { name: 'Actions of WEB-102' }).click();
    await expect.element(screen.getByRole('menuitem', { name: 'Open' })).toBeVisible();
    // NTF-014: watching needs only reading the issue, so it stays offered.
    await expect.element(screen.getByRole('menuitem', { name: 'Watch' })).toBeVisible();
    expect(screen.getByRole('menuitem').elements()).toHaveLength(2);
  });
});
