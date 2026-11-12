// Logic of the store. Reactivity needs the client build of Svelte: entities.browser.svelte.test.ts.
import { afterEach, describe, expect, it, vi } from 'vitest';

import { EntityStore, EntityTable } from './entities.ts';

interface Issue {
  id: number;
  version: number;
  summary: string;
  tags?: string[];
  description?: string;
}

function issue(id: number, version: number, summary: string, extra: Partial<Issue> = {}): Issue {
  return { id, version, summary, ...extra };
}

describe('EntityTable', () => {
  it('keeps the newer version and ignores older data', () => {
    const table = new EntityTable<Issue>('issue');
    table.upsert(issue(1, 3, 'Third'));
    table.upsert(issue(1, 2, 'Second'));
    expect(table.get(1)).toMatchObject({ version: 3, summary: 'Third' });
    table.upsert(issue(1, 4, 'Fourth'));
    expect(table.get(1)).toMatchObject({ version: 4, summary: 'Fourth' });
  });

  it('adds fields of the same version and drops fields older than a new version', () => {
    const table = new EntityTable<Issue>('issue');
    table.upsert(issue(1, 1, 'Short'));
    table.upsert(issue(1, 1, 'Short', { description: 'Full form' }));
    expect(table.get(1)?.description).toBe('Full form');
    table.upsert(issue(1, 1, 'Short'));
    expect(table.get(1)?.description).toBe('Full form');

    table.upsert(issue(1, 2, 'Renamed'));
    expect(table.get(1)).toEqual(issue(1, 2, 'Renamed'));
  });

  it('updates rows in place and does not keep references to the caller’s data', () => {
    const table = new EntityTable<Issue>('issue');
    const loaded = issue(1, 1, 'Before', { tags: ['a'] });
    const row = table.upsert(loaded);
    loaded.tags?.push('changed outside');
    expect(row.tags).toEqual(['a']);

    table.upsert(issue(1, 2, 'After', { tags: ['a', 'b'] }));
    expect(table.get(1)).toBe(row);
    expect(row).toMatchObject({ version: 2, summary: 'After', tags: ['a', 'b'] });
  });

  it('applies a patch only to the version right before it', () => {
    const table = new EntityTable<Issue>('issue');
    expect(table.applyPatch(1, 2, { summary: 'x' })).toBe('missing');

    table.upsert(issue(1, 1, 'One'));
    expect(table.applyPatch(1, 2, { summary: 'Two', id: 99, version: 99 })).toBe('applied');
    expect(table.get(1)).toEqual(issue(1, 2, 'Two'));
    expect(table.applyPatch(1, 2, { summary: 'Again' })).toBe('current');
    expect(table.applyPatch(1, 1, { summary: 'Old' })).toBe('current');
    expect(table.applyPatch(1, 4, { summary: 'Gap' })).toBe('stale');
    expect(table.get(1)).toEqual(issue(1, 2, 'Two'));
  });

  it('removes rows', () => {
    const table = new EntityTable<Issue>('issue');
    table.upsertMany([issue(1, 1, 'One'), issue(2, 1, 'Two')]);
    expect(table.size).toBe(2);
    expect(table.remove(1)).toBe(true);
    expect(table.remove(1)).toBe(false);
    expect(table.has(1)).toBe(false);
    expect(table.get(2)?.summary).toBe('Two');
  });
});

describe('EntityTable forms', () => {
  it('keeps the better form of one version and takes the form of a newer one', () => {
    const table = new EntityTable<Issue>('issue');
    table.upsert(issue(1, 1, 'One'), 'full');
    table.upsert(issue(1, 1, 'One'), 'short');
    expect(table.form(1)).toBe('full');
    table.upsert(issue(1, 2, 'Two'), 'short');
    expect(table.form(1)).toBe('short');
    table.upsert(issue(1, 2, 'Two', { description: 'Long' }), 'full');
    expect(table.form(1)).toBe('full');
  });

  it('reports a downgrade only while someone needs the full form', () => {
    const downgraded: number[] = [];
    const table = new EntityTable<Issue>('issue', { onDowngrade: (id) => downgraded.push(id) });
    table.upsert(issue(1, 1, 'One'), 'full');
    table.upsert(issue(1, 2, 'Two'), 'short');
    expect(downgraded).toEqual([]);

    table.upsert(issue(1, 3, 'Three'), 'full');
    const release = table.retain(1, 'full');
    table.upsert(issue(1, 4, 'Four'), 'short');
    expect(downgraded).toEqual([1]);

    release();
    table.upsert(issue(1, 5, 'Five'), 'full');
    table.upsert(issue(1, 6, 'Six'), 'short');
    expect(downgraded).toEqual([1]);
  });
});

describe('EntityTable local edits', () => {
  it('keeps optimistic values against data of the same version', () => {
    const table = new EntityTable<Issue>('issue');
    table.upsert(issue(1, 1, 'Server'));
    const edit = table.beginLocal(1, { summary: 'Mine' });
    table.upsert(issue(1, 1, 'Server', { description: 'Loaded' }), 'full');
    expect(table.get(1)).toMatchObject({ summary: 'Mine', description: 'Loaded' });
    expect(edit?.superseded.size).toBe(0);

    edit?.end();
    table.upsert(issue(1, 1, 'Server'));
    expect(table.get(1)?.summary).toBe('Server');
  });

  it('marks fields a newer version wrote as superseded', () => {
    const table = new EntityTable<Issue>('issue');
    table.upsert(issue(1, 1, 'Server', { description: 'Old' }));
    const edit = table.beginLocal(1, { summary: 'Mine', description: 'Mine too' });
    table.applyPatch(1, 2, { description: 'Theirs' });
    expect([...(edit?.superseded ?? [])]).toEqual(['description']);
    expect(table.get(1)).toMatchObject({ version: 2, summary: 'Mine', description: 'Theirs' });

    table.upsert(issue(1, 3, 'Replaced'));
    expect([...(edit?.superseded ?? [])].sort()).toEqual(['description', 'summary']);
  });

  it('cannot edit a row that is not loaded', () => {
    expect(new EntityTable<Issue>('issue').beginLocal(1, { summary: 'x' })).toBeNull();
  });
});

describe('EntityTable retention', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('evicts rows nobody retains after the delay', () => {
    vi.useFakeTimers();
    const table = new EntityTable<Issue>('issue', { evictAfterMs: 1000 });
    table.upsert(issue(1, 1, 'Unseen'));
    table.upsert(issue(2, 1, 'Shown'));
    const release = table.retain(2);
    vi.advanceTimersByTime(999);
    expect(table.has(1)).toBe(true);
    vi.advanceTimersByTime(1);
    expect(table.has(1)).toBe(false);
    expect(table.has(2)).toBe(true);

    release();
    vi.advanceTimersByTime(500);
    const again = table.retain(2); // back before the delay ran out
    vi.advanceTimersByTime(5000);
    expect(table.has(2)).toBe(true);
    again();
    again(); // a second call changes nothing
    vi.advanceTimersByTime(1000);
    expect(table.has(2)).toBe(false);
  });
});

describe('EntityStore', () => {
  it('creates a table per kind on first use', () => {
    const store = new EntityStore<{ issue: Issue }>();
    expect(store.find('issue')).toBeUndefined();
    const table = store.table('issue');
    expect(store.table('issue')).toBe(table);
    expect(store.find('issue')).toBe(table);

    table.upsert(issue(1, 1, 'One'));
    store.clear();
    expect(table.size).toBe(0);
  });
});
