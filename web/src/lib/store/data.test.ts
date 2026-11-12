// Logic of the data layer with fake timers. Retention by reading handles in components needs the
// client build of Svelte: data.browser.svelte.test.ts.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiError } from '#lib/api/client.ts';

import { ConflictError, DataStore, type EditRequest } from './data.ts';
import type { Form } from './entities.ts';
import { LIVE_BATCH_MS, type LiveSource, type Page } from './queries.ts';
import type { TopicHandlers } from '#lib/live/connection.ts';

interface Issue {
  id: number;
  version: number;
  summary: string;
  state: string;
  description?: string;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}

/** A server holding issues; `load` answers in the asked form. */
function setup() {
  const server = new Map<number, Issue>();
  const loads: { ids: number[]; form: Form }[] = [];
  const data = new DataStore<{ issue: Issue }>({ evictAfterMs: 60_000, staleTimeMs: 10_000 });
  const shape = (issue: Issue, form: Form): Issue => {
    if (form === 'full') return { ...issue };
    const short: Issue = { ...issue };
    delete short.description;
    return short;
  };
  data.defineKind('issue', {
    load: (ids, form) => {
      loads.push({ ids, form });
      return Promise.resolve(
        ids.flatMap((id) => {
          const issue = server.get(id);
          return issue ? [shape(issue, form)] : [];
        }),
      );
    },
  });
  const put = (issue: Issue): void => {
    server.set(issue.id, issue);
  };
  return { data, server, loads, put, issues: data.entities.table('issue') };
}

function issue(id: number, version = 1, extra: Partial<Issue> = {}): Issue {
  return {
    id,
    version,
    summary: `Issue ${String(id)}`,
    state: 'open',
    description: 'Long',
    ...extra,
  };
}

/** A page of a list endpoint: short forms, without the description. */
function page(ids: number[], next: string | null): Page<Issue> {
  return {
    items: ids.map((id) => ({ id, version: 1, summary: `Issue ${String(id)}`, state: 'open' })),
    next_cursor: next,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('ensureList', () => {
  it('keeps only IDs: items are the rows of the table', async () => {
    const { data, issues } = setup();
    const list = await data.ensureList('issue', {
      key: ['issues', 'open'],
      fetch: () => Promise.resolve(page([1, 2], null)),
    });
    expect(list.ids).toEqual([1, 2]);
    expect(list.items[0]).toBe(issues.get(1));
    expect(issues.form(1)).toBe('short');
    expect(issues.get(1)?.description).toBeUndefined();
  });

  it('arranges the shown rows in place, leaving out IDs it does not hold', async () => {
    const { data } = setup();
    const list = await data.ensureList('issue', {
      key: ['issues', 'arranged'],
      fetch: () => Promise.resolve(page([1, 2, 3], null)),
    });
    list.arrange([3, 1, 9]);
    expect(list.ids).toEqual([3, 1]);
    expect(list.items.map((one) => one.id)).toEqual([3, 1]);
  });

  it('sends one request for the same key and gives every caller the same promise', async () => {
    const { data } = setup();
    const answer = deferred<Page<Issue>>();
    const fetch = vi.fn(() => answer.promise);
    const first = data.ensureList('issue', { key: ['issues', { q: 'a', sort: 'id' }], fetch });
    const second = data.ensureList('issue', { key: ['issues', { sort: 'id', q: 'a' }], fetch });
    expect(second).toBe(first);
    expect(fetch).toHaveBeenCalledTimes(1);
    answer.resolve(page([1], null));
    expect((await first).ids).toEqual([1]);
    expect(data.ensureList('issue', { key: ['issues', { q: 'a', sort: 'id' }], fetch })).toBe(
      first,
    );
  });

  it('shows cached data at once and reloads it quietly when stale', async () => {
    const { data } = setup();
    const fetch = vi.fn((): Promise<Page<Issue>> => Promise.resolve(page([1, 2], null)));
    const spec = { key: ['issues'], fetch, staleTimeMs: 5000 };
    const list = await data.ensureList('issue', spec);

    vi.advanceTimersByTime(5000);
    await data.ensureList('issue', spec);
    expect(fetch).toHaveBeenCalledTimes(1); // still fresh

    fetch.mockImplementationOnce(() => Promise.resolve(page([3, 1], null)));
    vi.advanceTimersByTime(1);
    const again = data.ensureList('issue', spec);
    expect(list.ids).toEqual([1, 2]); // the old result shows while reloading
    expect(list.refreshing).toBe(true);
    expect(await again).toBe(list);
    await vi.waitFor(() => {
      expect(list.ids).toEqual([3, 1]);
    });
    expect(fetch).toHaveBeenCalledTimes(2);

    // A failed quiet reload keeps the data and reports the error.
    fetch.mockImplementationOnce(() => Promise.reject(new Error('offline')));
    vi.advanceTimersByTime(5001);
    await data.ensureList('issue', spec);
    await vi.waitFor(() => {
      expect(list.error).toBeInstanceOf(Error);
    });
    expect(list.ids).toEqual([3, 1]);
  });

  it('does not cache a failed first load', async () => {
    const { data } = setup();
    const fetch = vi.fn((): Promise<Page<Issue>> => Promise.reject(new Error('down')));
    await expect(data.ensureList('issue', { key: ['x'], fetch })).rejects.toThrow('down');
    fetch.mockImplementationOnce(() => Promise.resolve(page([4], null)));
    expect((await data.ensureList('issue', { key: ['x'], fetch })).ids).toEqual([4]);
  });

  it('appends the IDs of next pages, once per request', async () => {
    const { data } = setup();
    const pages: Record<string, Page<Issue>> = {
      first: page([1, 2], 'c2'),
      c2: page([2, 3], 'c3'), // the list shifted: 2 is not repeated
      c3: page([4], null),
    };
    const fetch = vi.fn((cursor: string | null) =>
      Promise.resolve(pages[cursor ?? 'first'] ?? page([], null)),
    );
    const list = await data.ensureList('issue', { key: ['paged'], fetch });
    expect(list.hasMore).toBe(true);

    const more = list.loadMore();
    expect(list.loadMore()).toBe(more);
    await more;
    expect(list.ids).toEqual([1, 2, 3]);
    await list.loadMore();
    expect(list.ids).toEqual([1, 2, 3, 4]);
    expect(list.hasMore).toBe(false);
    await list.loadMore();
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it('drops a next page that belongs to a list reloaded meanwhile', async () => {
    const { data } = setup();
    const next = deferred<Page<Issue>>();
    const fetch = vi.fn((cursor: string | null) =>
      cursor === null ? Promise.resolve(page([1], 'c2')) : next.promise,
    );
    const list = await data.ensureList('issue', { key: ['shifting'], fetch });
    const more = list.loadMore();
    fetch.mockImplementationOnce(() => Promise.resolve(page([9], 'other')));
    await list.refresh();
    next.resolve(page([2], null));
    await more;
    expect(list.ids).toEqual([9]);
    expect(list.hasMore).toBe(true);
  });
});

describe('eviction', () => {
  it('evicts a preloaded list nobody showed, then its rows', async () => {
    const { data, issues } = setup();
    const fetch = vi.fn((): Promise<Page<Issue>> => Promise.resolve(page([1], null)));
    await data.ensureList('issue', { key: ['preloaded'], fetch });

    vi.advanceTimersByTime(60_000); // the list goes; its rows lose their holder
    expect(issues.has(1)).toBe(true);
    vi.advanceTimersByTime(60_000);
    expect(issues.has(1)).toBe(false);

    await data.ensureList('issue', { key: ['preloaded'], fetch });
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it('keeps a retained list and its rows; retaining again cancels a pending eviction', async () => {
    const { data, issues } = setup();
    const fetch = vi.fn((): Promise<Page<Issue>> => Promise.resolve(page([1, 2], null)));
    const list = await data.ensureList('issue', { key: ['shown'], fetch });
    const release = list.retain();
    vi.advanceTimersByTime(600_000);
    expect(issues.has(1)).toBe(true);

    release();
    vi.advanceTimersByTime(59_000);
    const again = list.retain();
    vi.advanceTimersByTime(600_000);
    expect(issues.has(2)).toBe(true);
    // Still the same result: the call only reloads it quietly, since it is stale by now.
    expect(await data.ensureList('issue', { key: ['shown'], fetch })).toBe(list);

    again();
    vi.advanceTimersByTime(120_000);
    expect(issues.has(1)).toBe(false);
  });

  it('keeps a row shown elsewhere when a list that held it goes', async () => {
    const { data, issues, put } = setup();
    put(issue(1));
    const fetch = (): Promise<Page<Issue>> => Promise.resolve(page([1], null));
    await data.ensureList('issue', { key: ['a'], fetch });
    const entity = await data.ensureEntity('issue', 1);
    const release = entity.retain();
    vi.advanceTimersByTime(300_000);
    expect(issues.has(1)).toBe(true);
    release();
    vi.advanceTimersByTime(60_000);
    expect(issues.has(1)).toBe(false);
  });
});

describe('QRY-086: queries follow their live topic', () => {
  /** A connection whose topic's events the test sends. */
  function source() {
    const topics = new Map<string, TopicHandlers>();
    const live: LiveSource = {
      subscribe(topic, handlers) {
        topics.set(topic, handlers);
        return () => topics.delete(topic);
      },
    };
    const send = (
      id: number,
      version: number,
      change: 'created' | 'updated' | 'deleted' = 'updated',
      kind = 'issue',
    ): void => {
      topics.get('list')?.event?.({
        id: version,
        change,
        entity: { kind, id, version },
        patch: null,
        topics: ['list'],
      });
    };
    return { live, topics, send };
  }

  async function shown() {
    const env = setup();
    for (const id of [1, 2, 3]) env.put(issue(id));
    const fetch = vi.fn((): Promise<Page<Issue>> => Promise.resolve(page([1, 2, 3], null)));
    const list = await env.data.ensureList('issue', { key: ['live'], fetch });
    env.loads.length = 0;
    return { ...env, list, fetch };
  }

  it('refetches the changed shown rows in one batch', async () => {
    const { list, loads, put, issues, fetch } = await shown();
    const { live, send } = source();
    list.follow(live, 'list');
    put(issue(3, 2, { summary: 'Renamed' }));
    put(issue(1, 2));
    send(3, 2);
    send(1, 2);
    send(3, 2);
    expect(loads).toEqual([]);
    await vi.advanceTimersByTimeAsync(LIVE_BATCH_MS);
    expect(loads).toEqual([{ ids: [1, 3], form: 'short' }]);
    expect(issues.get(3)?.summary).toBe('Renamed');
    expect(list.stale).toBe(false);
    expect(fetch).toHaveBeenCalledTimes(1); // the list itself is not reloaded
  });

  it('skips events the rows already have and other kinds', async () => {
    const { list, loads } = await shown();
    const { live, send } = source();
    list.follow(live, 'list');
    send(2, 1);
    send(9, 4, 'created', 'comment');
    await vi.advanceTimersByTimeAsync(LIVE_BATCH_MS);
    expect(loads).toEqual([]);
    expect(list.stale).toBe(false);
  });

  it('marks the list stale for other issues and keeps its rows until a reload', async () => {
    const { list, loads, fetch } = await shown();
    const { live, send } = source();
    list.follow(live, 'list');
    send(7, 1, 'created');
    expect(list.stale).toBe(true);
    await vi.advanceTimersByTimeAsync(LIVE_BATCH_MS);
    expect(loads).toEqual([]);
    expect(list.ids).toEqual([1, 2, 3]);
    expect(fetch).toHaveBeenCalledTimes(1);

    fetch.mockImplementationOnce(() => Promise.resolve(page([7, 1, 2, 3], null)));
    await list.refresh();
    expect(list.ids).toEqual([7, 1, 2, 3]);
    expect(list.stale).toBe(false);
  });

  it('keeps a deleted row listed, drops its data and offers the reload', async () => {
    const { list, server, issues } = await shown();
    const { live, send } = source();
    list.follow(live, 'list');
    server.delete(2);
    send(2, 2, 'deleted');
    expect(list.stale).toBe(true);
    await vi.advanceTimersByTimeAsync(LIVE_BATCH_MS);
    expect(issues.has(2)).toBe(false);
    expect(list.items.map((row) => row.id)).toEqual([1, 3]);
  });

  it('refetches every shown row after a failed catch-up, and stops when asked', async () => {
    const { list, loads } = await shown();
    const { live, topics, send } = source();
    const stop = list.follow(live, 'list');
    topics.get('list')?.resync?.();
    expect(list.stale).toBe(true);
    await vi.advanceTimersByTimeAsync(LIVE_BATCH_MS);
    expect(loads).toEqual([{ ids: [1, 2, 3], form: 'short' }]);

    send(1, 5);
    stop();
    expect(topics.size).toBe(0);
    await vi.advanceTimersByTimeAsync(LIVE_BATCH_MS);
    expect(loads).toHaveLength(1);
  });
});

describe('ensureEntity', () => {
  it('loads a missing entity once and returns a handle into the table', async () => {
    const { data, issues, loads, put } = setup();
    put(issue(7));
    const first = data.ensureEntity('issue', 7, { form: 'full' });
    expect(data.ensureEntity('issue', 7)).toBe(first); // a short need waits for the full load
    const handle = await first;
    expect(handle.current).toBe(issues.get(7));
    expect(handle.current?.description).toBe('Long');
    expect(loads).toEqual([{ ids: [7], form: 'full' }]);
  });

  it('answers from the cache and loads the full form only when needed', async () => {
    const { data, issues, loads, put } = setup();
    put(issue(1));
    await data.ensureList('issue', {
      key: ['l'],
      fetch: () => Promise.resolve(page([1], null)),
    });
    await data.ensureEntity('issue', 1);
    expect(loads).toEqual([]);

    await data.ensureEntity('issue', 1, { form: 'full' });
    expect(loads).toEqual([{ ids: [1], form: 'full' }]);
    expect(issues.form(1)).toBe('full');
    await data.ensureEntity('issue', 1, { form: 'full' });
    await data.ensureEntity('issue', 1);
    expect(loads).toHaveLength(1);
  });

  it('reloads a stale entity quietly in the form it holds', async () => {
    const { data, issues, loads, put } = setup();
    put(issue(1));
    await data.ensureEntity('issue', 1, { form: 'full' });
    put(issue(1, 2, { summary: 'Renamed' }));
    vi.advanceTimersByTime(10_001);
    const pending = data.ensureEntity('issue', 1);
    expect(issues.get(1)?.summary).toBe('Issue 1'); // cached data first
    expect((await pending).current).toBe(issues.get(1));
    await vi.waitFor(() => {
      expect(issues.get(1)?.summary).toBe('Renamed');
    });
    expect(loads.at(-1)).toEqual({ ids: [1], form: 'full' });
  });

  it('fetches the full form again after a newer short version replaced it', async () => {
    const { data, issues, loads, put } = setup();
    put(issue(1));
    const handle = await data.ensureEntity('issue', 1, { form: 'full' });
    put(issue(1, 2));
    issues.upsert(issue(1, 2), 'short'); // a list loaded the newer short form
    expect(issues.form(1)).toBe('short');
    await data.ensureEntity('issue', 1, { form: 'full' });
    expect(loads.at(-1)).toEqual({ ids: [1], form: 'full' });

    // While a view shows the full form, the downgrade reloads it at once.
    const release = handle.retain();
    put(issue(1, 3, { description: 'Newer' }));
    issues.upsert(issue(1, 3), 'short');
    await vi.waitFor(() => {
      expect(issues.form(1)).toBe('full');
    });
    expect(issues.get(1)?.description).toBe('Newer');
    release();
  });

  it('rejects with not_found and drops an entity the server no longer returns', async () => {
    const { data, issues, put, server } = setup();
    await expect(data.ensureEntity('issue', 5)).rejects.toMatchObject({ code: 'not_found' });

    put(issue(6));
    await data.ensureEntity('issue', 6);
    server.delete(6);
    await data.refetch('issue', [6]);
    expect(issues.has(6)).toBe(false);
  });

  it('lets a preloaded entity go unless a view retains it in time', async () => {
    const { data, issues, put } = setup();
    put(issue(1));
    put(issue(2));
    await data.ensureEntity('issue', 1); // preloaded on hover, never visited
    const visited = await data.ensureEntity('issue', 2);
    vi.advanceTimersByTime(30_000);
    const release = visited.retain(); // the page opened
    vi.advanceTimersByTime(30_000);
    expect(issues.has(1)).toBe(false);
    expect(issues.has(2)).toBe(true);
    release();
  });
});

describe('put', () => {
  it('stores entities of another answer by version, as a load does, without loading', async () => {
    const { data, issues, loads } = setup();
    issues.upsert(issue(1, 2), 'short');
    issues.upsert(issue(2, 1), 'short');

    data.put('issue', [issue(1, 1, { summary: 'Old' }), issue(2, 2, { summary: 'New' })], 'full');
    data.put('issue', [issue(3)], 'full');

    expect(issues.get(1)?.summary).not.toBe('Old');
    expect(issues.get(2)?.summary).toBe('New');
    expect(issues.form(2)).toBe('full');
    expect(issues.form(1)).toBe('short');
    // Rows not given stay; a full row needs no load.
    expect(issues.has(1)).toBe(true);
    await data.ensureEntity('issue', 3, { form: 'full' });
    expect(loads).toEqual([]);
  });
});

describe('update', () => {
  function editing() {
    const context = setup();
    context.put(issue(1));
    const sent: EditRequest<Issue>[] = [];
    return { ...context, sent };
  }

  it('shows the change at once and stores what the server saved', async () => {
    const { data, issues, sent } = editing();
    await data.ensureEntity('issue', 1);
    const answer = deferred<Issue>();
    const saving = data.update('issue', 1, { summary: 'Mine' }, (request) => {
      sent.push(request);
      return answer.promise;
    });
    expect(issues.get(1)).toMatchObject({ summary: 'Mine', version: 1 });
    answer.resolve(issue(1, 2, { summary: 'Mine' }));
    await saving;
    expect(sent).toEqual([{ changes: { summary: 'Mine' }, version: 1 }]);
    expect(issues.get(1)).toMatchObject({ summary: 'Mine', version: 2, description: 'Long' });
  });

  it('rolls back on failure', async () => {
    const { data, issues } = editing();
    await data.ensureEntity('issue', 1);
    const failure = new ApiError(500, null);
    await expect(
      data.update('issue', 1, { summary: 'Mine', state: 'done' }, () => Promise.reject(failure)),
    ).rejects.toBe(failure);
    expect(issues.get(1)).toMatchObject({ summary: 'Issue 1', state: 'open', version: 1 });
  });

  it('never rolls back over a newer version that arrived meanwhile', async () => {
    const { data, issues } = editing();
    await data.ensureEntity('issue', 1);
    const answer = deferred<Issue>();
    const saving = data.update(
      'issue',
      1,
      { summary: 'Mine', state: 'done' },
      () => answer.promise,
    );
    // Someone else set the same state; a live event brings version 2.
    issues.applyPatch(1, 2, { state: 'done' });
    answer.reject(new ApiError(500, null));
    await expect(saving).rejects.toBeInstanceOf(ApiError);
    expect(issues.get(1)).toMatchObject({ version: 2, state: 'done', summary: 'Issue 1' });
  });

  it('keeps a later local edit of the same field when an earlier one fails', async () => {
    const { data, issues } = editing();
    await data.ensureEntity('issue', 1);
    const first = deferred<Issue>();
    const second = deferred<Issue>();
    const a = data.update('issue', 1, { summary: 'A' }, () => first.promise);
    const b = data.update('issue', 1, { summary: 'B' }, () => second.promise);
    first.reject(new ApiError(500, null));
    await expect(a).rejects.toBeInstanceOf(ApiError);
    expect(issues.get(1)?.summary).toBe('B');
    second.resolve(issue(1, 2, { summary: 'B' }));
    await b;
    expect(issues.get(1)).toMatchObject({ summary: 'B', version: 2 });
  });

  it('sends again on the new version when the other change touched other fields', async () => {
    const { data, issues, put, sent } = editing();
    await data.ensureEntity('issue', 1);
    put(issue(1, 2, { state: 'in progress' })); // someone changed the state
    const saved = await data.update('issue', 1, { summary: 'Mine' }, (request) => {
      sent.push(request);
      if (request.version === 1) return Promise.reject(new ApiError(409, null));
      return Promise.resolve(issue(1, 3, { summary: 'Mine', state: 'in progress' }));
    });
    expect(sent.map((request) => request.version)).toEqual([1, 2]);
    expect(saved).toBe(issues.get(1));
    expect(issues.get(1)).toMatchObject({ version: 3, summary: 'Mine', state: 'in progress' });
  });

  it('keeps the server state and reports a conflict on the same field', async () => {
    const { data, issues, put } = editing();
    await data.ensureEntity('issue', 1);
    put(issue(1, 2, { summary: 'Theirs' }));
    const send = vi.fn(() => Promise.reject(new ApiError(409, null)));
    const failure = await data
      .update('issue', 1, { summary: 'Mine' }, send)
      .catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(ConflictError);
    expect(failure).toMatchObject({
      kind: 'issue',
      id: 1,
      server: { version: 2, summary: 'Theirs' },
      attempted: { summary: 'Mine' },
    });
    expect(send).toHaveBeenCalledTimes(1);
    expect(issues.get(1)).toMatchObject({ version: 2, summary: 'Theirs' });
  });

  it('retries once only', async () => {
    const { data, issues, put } = editing();
    await data.ensureEntity('issue', 1);
    put(issue(1, 2, { state: 'in progress' }));
    const send = vi.fn((request: EditRequest<Issue>) => {
      put(issue(1, request.version + 1, { state: `changed ${String(request.version)}` }));
      return Promise.reject(new ApiError(409, null));
    });
    await expect(data.update('issue', 1, { summary: 'Mine' }, send)).rejects.toBeInstanceOf(
      ConflictError,
    );
    expect(send).toHaveBeenCalledTimes(2);
    expect(issues.get(1)?.summary).toBe('Issue 1');
  });
});
