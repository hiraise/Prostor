// Handles retain what components read, for as long as they read it (needs the client build of
// Svelte, so it runs in the browser).
import { flushSync, tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { DataStore } from './data.ts';

interface Note {
  id: number;
  version: number;
  text: string;
}

function setup() {
  const data = new DataStore<{ note: Note }>({ evictAfterMs: 1000 });
  data.defineKind('note', {
    load: (ids) =>
      Promise.resolve(ids.map((id) => ({ id, version: 1, text: `Note ${String(id)}` }))),
  });
  return { data, notes: data.entities.table('note') };
}

/** Runs `read` in an effect, like a component template does, until `stop`. */
function show(read: () => unknown): { seen: unknown[]; stop: () => Promise<void> } {
  const seen: unknown[] = [];
  const stop = $effect.root(() => {
    $effect(() => {
      seen.push(read());
    });
  });
  flushSync();
  return {
    seen,
    stop: async () => {
      stop();
      // Svelte releases a subscription a microtask after its last reader goes.
      await tick();
    },
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

it('keeps an entity while a component reads it and lets it go after the delay', async () => {
  const { data, notes } = setup();
  const handle = await data.ensureEntity('note', 1);
  const view = show(() => handle.current?.text);

  vi.advanceTimersByTime(5000);
  expect(notes.has(1)).toBe(true);
  notes.applyPatch(1, 2, { text: 'Edited' });
  flushSync();
  expect(view.seen).toEqual(['Note 1', 'Edited']);

  await view.stop();
  vi.advanceTimersByTime(999);
  expect(notes.has(1)).toBe(true);
  vi.advanceTimersByTime(1);
  expect(notes.has(1)).toBe(false);
});

it('keeps a list and its rows while shown and follows its IDs', async () => {
  const { data, notes } = setup();
  let next: string | null = 'more';
  const list = await data.ensureList('note', {
    key: ['notes'],
    fetch: (cursor) => {
      const page = cursor === null ? [1, 2] : [3];
      const items = page.map((id) => ({ id, version: 1, text: `Note ${String(id)}` }));
      const answer = { items, next_cursor: next };
      next = null;
      return Promise.resolve(answer);
    },
  });
  const view = show(() => list.items.map((note) => note.text).join(', '));

  await list.loadMore();
  flushSync();
  expect(view.seen).toEqual(['Note 1, Note 2', 'Note 1, Note 2, Note 3']);
  vi.advanceTimersByTime(10_000);
  expect(notes.size).toBe(3);

  await view.stop();
  vi.advanceTimersByTime(1000); // the list goes
  vi.advanceTimersByTime(1000); // then its rows
  expect(notes.size).toBe(0);
});
