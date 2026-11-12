// Reactivity of the store, in a real browser: Node tests get the server build of Svelte, where
// effects do not run.
import { flushSync } from 'svelte';
import { describe, expect, it } from 'vitest';

import { EntityTable } from './entities.ts';

interface Issue {
  id: number;
  version: number;
  summary: string;
  tags?: string[];
}

function issue(id: number, version: number, summary: string): Issue {
  return { id, version, summary };
}

/** Runs `read` in an effect and counts how often it runs. */
function track(read: () => unknown): { runs: () => number; stop: () => void } {
  let runs = 0;
  const stop = $effect.root(() => {
    $effect(() => {
      read();
      runs += 1;
    });
  });
  flushSync();
  return { runs: () => runs, stop };
}

describe('EntityTable reactivity', () => {
  it('wakes only readers of the changed row and field', () => {
    const table = new EntityTable<Issue>('issue');
    table.upsertMany([issue(1, 1, 'One'), issue(2, 1, 'Two')]);

    const first = track(() => table.get(1)?.summary);
    const second = track(() => table.get(2)?.summary);
    const firstTags = track(() => table.get(1)?.tags);

    table.upsert(issue(2, 2, 'Two, renamed'));
    flushSync();
    expect(second.runs()).toBe(2);
    expect(first.runs()).toBe(1);

    // Same values under a new version: the summary readers are not woken.
    table.applyPatch(1, 2, { summary: 'One', tags: ['x'] });
    flushSync();
    expect(first.runs()).toBe(1);
    expect(firstTags.runs()).toBe(2);

    table.applyPatch(1, 3, { summary: 'One, renamed' });
    flushSync();
    expect(first.runs()).toBe(2);
    expect(second.runs()).toBe(2);

    table.remove(1);
    flushSync();
    expect(first.runs()).toBe(3);
    expect(second.runs()).toBe(2);

    for (const reader of [first, second, firstTags]) reader.stop();
  });

  it('wakes a reader waiting for a row that arrives later', () => {
    const table = new EntityTable<Issue>('issue');
    let seen: string | undefined;
    const reader = track(() => (seen = table.get(5)?.summary));
    table.upsert(issue(5, 1, 'Arrived'));
    flushSync();
    expect(seen).toBe('Arrived');
    expect(reader.runs()).toBe(2);
    reader.stop();
  });
});
