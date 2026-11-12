import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { LiveEvent } from '#lib/api/schema.gen.ts';
import { EntityStore } from '#lib/store/entities.ts';

import { LiveConnection, type LiveSocketCallbacks } from './connection.ts';
import { LiveSync } from './sync.ts';

interface Issue {
  id: number;
  version: number;
  summary: string;
}

function change(
  change: LiveEvent['change'],
  id: number,
  version: number,
  patch: LiveEvent['patch'] = null,
  kind = 'issue',
): LiveEvent {
  return { id: 1000 + version, topics: ['list:abc'], entity: { kind, id, version }, change, patch };
}

function setup() {
  const store = new EntityStore<{ issue: Issue }>();
  const issues = store.table('issue');
  const refetch = vi.fn<(kind: string, ids: number[]) => Promise<void>>(() => Promise.resolve());
  const sync = new LiveSync({ store, refetch });
  return { store, issues, refetch, sync };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('LiveSync', () => {
  it('applies a patch to the version before it', () => {
    const { issues, refetch, sync } = setup();
    issues.upsert({ id: 1, version: 1, summary: 'Old' });
    sync.handle(change('updated', 1, 2, { summary: 'New' }));
    expect(issues.get(1)).toEqual({ id: 1, version: 2, summary: 'New' });
    vi.runAllTimers();
    expect(refetch).not.toHaveBeenCalled();
  });

  it('refetches stale rows and changes without a patch, batched per kind', () => {
    const { issues, refetch, sync } = setup();
    issues.upsertMany([1, 2, 3, 4].map((id) => ({ id, version: 1, summary: `#${String(id)}` })));

    sync.handle(change('updated', 3, 2)); // no patch
    sync.handle(change('updated', 1, 5, { summary: 'gap' })); // missed versions
    sync.handle(change('updated', 2, 2, { summary: 'fine' })); // applied
    sync.handle(change('updated', 3, 3)); // already queued
    expect(refetch).not.toHaveBeenCalled();

    vi.advanceTimersByTime(20);
    expect(refetch).toHaveBeenCalledTimes(1);
    expect(refetch).toHaveBeenCalledWith('issue', [1, 3]);
    expect(issues.get(2)?.summary).toBe('fine');
  });

  it('ignores events it already has and entities nobody keeps', () => {
    const { issues, refetch, sync } = setup();
    issues.upsert({ id: 1, version: 4, summary: 'Current' });
    sync.handle(change('updated', 1, 4));
    sync.handle(change('updated', 1, 3, { summary: 'Older' }));
    sync.handle(change('updated', 9, 2)); // not in the store
    sync.handle(change('updated', 1, 9, null, 'article')); // kind nobody stores
    vi.runAllTimers();
    expect(refetch).not.toHaveBeenCalled();
    expect(issues.get(1)?.summary).toBe('Current');
    expect(issues.has(9)).toBe(false);
  });

  it('keeps created entities that arrive whole', () => {
    const { issues, sync } = setup();
    sync.handle(change('created', 5, 1, { summary: 'Fresh' }));
    sync.handle(change('created', 6, 1)); // no data: a view of the topic loads it if it wants
    expect(issues.get(5)).toEqual({ id: 5, version: 1, summary: 'Fresh' });
    expect(issues.has(6)).toBe(false);
  });

  it('removes deleted entities and forgets their pending fetch', () => {
    const { issues, refetch, sync } = setup();
    issues.upsert({ id: 1, version: 1, summary: 'Doomed' });
    sync.handle(change('updated', 1, 2));
    sync.handle(change('deleted', 1, 2));
    expect(issues.has(1)).toBe(false);
    vi.runAllTimers();
    expect(refetch).not.toHaveBeenCalled();
  });

  it('reports failed refetches', async () => {
    const onError = vi.fn();
    const store = new EntityStore<{ issue: Issue }>();
    store.table('issue').upsert({ id: 1, version: 1, summary: 'x' });
    const sync = new LiveSync({
      store,
      refetch: () => Promise.reject(new Error('offline')),
      onError,
    });
    sync.handle(change('updated', 1, 2));
    sync.flush();
    await vi.waitFor(() => {
      expect(onError).toHaveBeenCalledTimes(1);
    });
  });

  it('follows a topic: events update the store, the view hears the rest', () => {
    const { issues, sync } = setup();
    issues.upsert({ id: 1, version: 1, summary: 'Old' });
    let callbacks: LiveSocketCallbacks | null = null;
    const connection = new LiveConnection({
      url: () => 'ws://test',
      createSocket: (_url, socketCallbacks) => {
        callbacks = socketCallbacks;
        return { send: () => undefined, close: () => undefined };
      },
    });
    const seen = vi.fn();
    const resync = vi.fn();
    const stop = sync.follow(connection, 'issue:1', { event: seen, resync });
    const socket = callbacks as LiveSocketCallbacks | null;
    if (!socket) throw new Error('no socket');
    socket.open();

    const event = { ...change('updated', 1, 2, { summary: 'New' }), topics: ['issue:1'] };
    socket.message(JSON.stringify({ type: 'event', ...event }));
    socket.message(JSON.stringify({ type: 'resync', topics: ['issue:1'] }));
    expect(issues.get(1)?.summary).toBe('New');
    expect(seen).toHaveBeenCalledWith({ type: 'event', ...event });
    expect(resync).toHaveBeenCalledTimes(1);
    stop();
    connection.close();
  });
});
