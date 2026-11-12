import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { LiveClientMessage, LiveEvent } from '#lib/api/schema.gen.ts';

import {
  CloseCode,
  LiveConnection,
  parseMessage,
  type LiveConnectionOptions,
  type LiveEnvironment,
  type LiveSocketCallbacks,
} from './connection.ts';

/** A socket the test opens, feeds and closes by hand. */
class FakeSocket {
  readonly sent: LiveClientMessage[] = [];
  closed = false;

  constructor(
    readonly url: string,
    private readonly callbacks: LiveSocketCallbacks,
  ) {}

  send(data: string): void {
    this.sent.push(JSON.parse(data) as LiveClientMessage);
  }

  close(): void {
    this.closed = true;
  }

  open(): void {
    this.callbacks.open();
  }

  receive(message: unknown): void {
    this.callbacks.message(typeof message === 'string' ? message : JSON.stringify(message));
  }

  /** The server or the network closed the socket. */
  drop(code = 1006): void {
    this.callbacks.close(code);
  }
}

class FakeEnvironment implements LiveEnvironment {
  isOnline = true;
  listeners = new Set<(change: 'online' | 'offline' | 'visible' | 'hidden') => void>();

  online(): boolean {
    return this.isOnline;
  }

  listen(listener: (change: 'online' | 'offline' | 'visible' | 'hidden') => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(change: 'online' | 'offline' | 'visible' | 'hidden'): void {
    for (const listener of this.listeners) listener(change);
  }
}

function setup(options: LiveConnectionOptions = {}) {
  const sockets: FakeSocket[] = [];
  const environment = new FakeEnvironment();
  const connection = new LiveConnection({
    url: () => 'ws://test/api/v1/live',
    createSocket: (url, callbacks) => {
      const socket = new FakeSocket(url, callbacks);
      sockets.push(socket);
      return socket;
    },
    environment,
    // No jitter: delays are exactly half of the ceiling.
    random: () => 0,
    ...options,
  });
  const last = (): FakeSocket => {
    const socket = sockets.at(-1);
    if (!socket) throw new Error('no socket was opened');
    return socket;
  };
  return { connection, sockets, environment, last };
}

function event(id: number, topics: string[], entityId = 7): LiveEvent {
  return {
    id,
    topics,
    entity: { kind: 'issue', id: entityId, version: id },
    change: 'updated',
    patch: null,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('LiveConnection', () => {
  it('connects only when the first topic is subscribed', () => {
    const { connection, sockets, last } = setup();
    expect(connection.status).toBe('idle');
    expect(sockets).toHaveLength(0);

    connection.subscribe('issue:7', {});
    expect(sockets).toHaveLength(1);
    expect(last().url).toBe('ws://test/api/v1/live');
    expect(connection.status).toBe('connecting');

    last().open();
    expect(connection.status).toBe('open');
    expect(last().sent).toEqual([{ type: 'subscribe', topics: ['issue:7'], since: null }]);
  });

  it('shares one server subscription between subscribers of a topic', () => {
    const { connection, last } = setup();
    const first = connection.subscribe('issue:7', {});
    last().open();
    const second = connection.subscribe('issue:7', {});
    connection.subscribe('inbox', {});
    expect(last().sent).toEqual([
      { type: 'subscribe', topics: ['issue:7'], since: null },
      { type: 'subscribe', topics: ['inbox'], since: null },
    ]);

    first();
    first(); // a second call does nothing
    expect(last().sent).toHaveLength(2);
    second();
    expect(last().sent.at(-1)).toEqual({ type: 'unsubscribe', topics: ['issue:7'] });
    expect(connection.topics).toEqual(['inbox']);
  });

  it('delivers events to the listeners of their topics, once per listener', () => {
    const { connection, last } = setup();
    const issue = vi.fn();
    const board = vi.fn();
    const both = { event: vi.fn() };
    connection.subscribe('issue:7', { event: issue });
    connection.subscribe('board:1:2', { event: board });
    connection.subscribe('issue:7', both);
    connection.subscribe('board:1:2', both);
    last().open();

    last().receive({ type: 'event', ...event(10, ['issue:7', 'board:1:2']) });
    expect(issue).toHaveBeenCalledTimes(1);
    expect(board).toHaveBeenCalledTimes(1);
    expect(both.event).toHaveBeenCalledTimes(2); // two subscriptions, one call each
    expect(issue).toHaveBeenCalledWith({ type: 'event', ...event(10, ['issue:7', 'board:1:2']) });
  });

  it('calls subscribed, denied and resync callbacks', () => {
    const { connection, last } = setup();
    const handlers = { subscribed: vi.fn(), denied: vi.fn(), resync: vi.fn() };
    const other = { subscribed: vi.fn(), denied: vi.fn(), resync: vi.fn() };
    connection.subscribe('issue:7', handlers);
    connection.subscribe('issue:9', other);
    last().open();

    last().receive({ type: 'subscribed', topics: ['issue:7'], head: 5 });
    last().receive({ type: 'denied', topics: ['issue:9'] });
    last().receive({ type: 'resync', topics: ['issue:7'] });
    expect(handlers.subscribed).toHaveBeenCalledTimes(1);
    expect(handlers.resync).toHaveBeenCalledTimes(1);
    expect(handlers.denied).not.toHaveBeenCalled();
    expect(other.denied).toHaveBeenCalledTimes(1);
    expect(other.subscribed).not.toHaveBeenCalled();
  });

  it('reconnects with a growing delay and resumes from the last event', () => {
    const { connection, sockets, last } = setup();
    connection.subscribe('issue:7', {});
    connection.subscribe('inbox', {});
    last().open();
    last().receive({ type: 'subscribed', topics: ['issue:7', 'inbox'], head: 40 });
    last().receive({ type: 'event', ...event(42, ['issue:7']) });

    last().drop();
    expect(connection.status).toBe('reconnecting');
    vi.advanceTimersByTime(249);
    expect(sockets).toHaveLength(1);
    vi.advanceTimersByTime(1); // 500 ms ceiling, half of it without jitter
    expect(sockets).toHaveLength(2);
    expect(connection.status).toBe('reconnecting');

    last().drop(); // failed to open
    vi.advanceTimersByTime(499);
    expect(sockets).toHaveLength(2);
    vi.advanceTimersByTime(1); // 1 s ceiling
    expect(sockets).toHaveLength(3);

    last().open();
    expect(connection.status).toBe('open');
    expect(last().sent).toEqual([{ type: 'subscribe', topics: ['issue:7', 'inbox'], since: 42 }]);
  });

  it('live_session_ended_stops_reconnecting_and_ends_the_session', () => {
    const onSessionEnded = vi.fn();
    const { connection, sockets, last } = setup({ onSessionEnded });
    const stop = connection.subscribe('issue:7', {});
    last().open();
    last().drop(CloseCode.SESSION_ENDED);
    expect(connection.status).toBe('idle');
    expect(onSessionEnded).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(60_000);
    expect(sockets).toHaveLength(1);
    stop(); // a view leaving afterwards changes nothing
    expect(connection.status).toBe('idle');
  });

  it.each([
    ['restart', CloseCode.RESTART],
    ['too slow', CloseCode.TOO_SLOW],
    ['policy', CloseCode.POLICY],
  ])('live_close_%s_reconnects_and_replays', (_name, code) => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const onSessionEnded = vi.fn();
    const { connection, sockets, last } = setup({ onSessionEnded });
    connection.subscribe('issue:7', {});
    last().open();
    last().receive({ type: 'subscribed', topics: ['issue:7'], head: 40 });
    last().drop(code);
    expect(connection.status).toBe('reconnecting');
    vi.advanceTimersByTime(250);
    expect(sockets).toHaveLength(2);
    last().open();
    expect(last().sent).toEqual([{ type: 'subscribe', topics: ['issue:7'], since: 40 }]);
    expect(onSessionEnded).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledTimes(code === CloseCode.POLICY ? 1 : 0);
    warn.mockRestore();
  });

  it('keeps the head of a subscription as its position when no event arrived', () => {
    const { connection, last } = setup();
    connection.subscribe('inbox', {});
    last().open();
    last().receive({ type: 'subscribed', topics: ['inbox'], head: 77 });
    last().drop();
    vi.advanceTimersByTime(250);
    last().open();
    expect(last().sent).toEqual([{ type: 'subscribe', topics: ['inbox'], since: 77 }]);
  });

  it('caps the delay and resets it after a stable connection', () => {
    const { connection, sockets, last } = setup({ retryMaxMs: 2_000 });
    connection.subscribe('inbox', {});
    for (let attempt = 0; attempt < 6; attempt += 1) {
      last().drop();
      vi.runOnlyPendingTimers();
    }
    const before = sockets.length;
    last().drop();
    vi.advanceTimersByTime(999);
    expect(sockets).toHaveLength(before);
    vi.advanceTimersByTime(1); // ceiling 2 s → 1 s without jitter
    expect(sockets).toHaveLength(before + 1);

    last().open();
    vi.advanceTimersByTime(10_000); // stable
    last().drop();
    vi.advanceTimersByTime(250); // back to the first delay
    expect(sockets).toHaveLength(before + 2);
  });

  it('adds jitter to the delay', () => {
    const { connection, sockets, last } = setup({ random: () => 0.8 });
    connection.subscribe('inbox', {});
    last().drop();
    vi.advanceTimersByTime(449); // 250 fixed + 0.8 × 250 random
    expect(sockets).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(2);
  });

  it('reports offline and reconnects at once when the network is back', () => {
    const { connection, sockets, environment, last } = setup();
    connection.subscribe('inbox', {});
    last().open();
    environment.isOnline = false;
    last().drop();
    expect(connection.status).toBe('offline');

    environment.isOnline = true;
    environment.emit('online');
    expect(sockets).toHaveLength(2);
    expect(connection.status).toBe('reconnecting');
  });

  it('checks the connection when a tab returns after a long time hidden', () => {
    const { connection, environment, last } = setup();
    connection.subscribe('inbox', {});
    last().open();
    environment.emit('hidden');
    vi.advanceTimersByTime(5_000);
    environment.emit('visible');
    expect(last().sent.filter((message) => message.type === 'ping')).toHaveLength(0);

    environment.emit('hidden');
    vi.setSystemTime(Date.now() + 120_000);
    environment.emit('visible');
    expect(last().sent.at(-1)).toEqual({ type: 'ping' });
  });

  it('pings and reconnects when no pong comes back', () => {
    const { connection, sockets, last } = setup();
    connection.subscribe('inbox', {});
    last().open();

    vi.advanceTimersByTime(25_000);
    expect(last().sent.at(-1)).toEqual({ type: 'ping' });
    last().receive({ type: 'pong' });
    vi.advanceTimersByTime(10_000);
    expect(sockets).toHaveLength(1);

    vi.advanceTimersByTime(15_000); // next ping
    expect(last().sent.filter((message) => message.type === 'ping')).toHaveLength(2);
    vi.advanceTimersByTime(10_000); // no pong
    expect(last().closed).toBe(true);
    expect(connection.status).toBe('reconnecting');
    vi.advanceTimersByTime(250);
    expect(sockets).toHaveLength(2);
  });

  it('closes after the last topic is gone and stays idle', () => {
    const { connection, sockets, last } = setup({ pingEveryMs: 1_000_000 });
    const stop = connection.subscribe('inbox', {});
    last().open();
    stop();
    vi.advanceTimersByTime(9_999);
    expect(last().closed).toBe(false);

    // A view that subscribes within the idle time reuses the socket.
    const again = connection.subscribe('issue:7', {});
    vi.advanceTimersByTime(20_000);
    expect(last().closed).toBe(false);
    again();
    vi.advanceTimersByTime(10_000);
    expect(last().closed).toBe(true);
    expect(connection.status).toBe('idle');

    last().drop(); // the late close callback of the old socket changes nothing
    vi.runOnlyPendingTimers();
    expect(sockets).toHaveLength(1);
    expect(connection.status).toBe('idle');
  });

  it('stops reconnecting when nobody needs the connection', () => {
    const { connection, sockets, last } = setup();
    const stop = connection.subscribe('inbox', {});
    last().drop();
    stop();
    expect(connection.status).toBe('idle');
    vi.runOnlyPendingTimers();
    expect(sockets).toHaveLength(1);
  });

  it('ignores malformed and unknown messages', () => {
    const { connection, last } = setup();
    const handlers = { event: vi.fn(), subscribed: vi.fn() };
    connection.subscribe('issue:7', handlers);
    last().open();

    last().receive('not json');
    last().receive({ type: 'event', id: 'x', topics: ['issue:7'] });
    last().receive({ type: 'event', ...event(1, ['issue:7']), change: 'renamed' });
    last().receive({ type: 'subscribed', topics: ['issue:7'] }); // no head
    last().receive({ type: 'surprise', topics: ['issue:7'] });
    last().receive([1, 2]);
    expect(handlers.event).not.toHaveBeenCalled();
    expect(handlers.subscribed).not.toHaveBeenCalled();
    expect(connection.status).toBe('open');
  });

  it('keeps delivering when a listener throws', () => {
    const errors: unknown[] = [];
    const { connection, last } = setup({ onError: (error) => errors.push(error) });
    const after = vi.fn();
    connection.subscribe('issue:7', {
      event: () => {
        throw new Error('broken view');
      },
    });
    connection.subscribe('issue:7', { event: after });
    last().open();
    last().receive({ type: 'event', ...event(3, ['issue:7']) });
    expect(after).toHaveBeenCalledTimes(1);
    expect(errors).toHaveLength(1);
  });
});

describe('parseMessage', () => {
  it('reads an event and fills a missing patch with null', () => {
    const raw: Record<string, unknown> = { type: 'event', ...event(5, ['issue:7']) };
    delete raw.patch;
    expect(parseMessage(JSON.stringify(raw))).toEqual({
      type: 'event',
      ...event(5, ['issue:7']),
    });
  });

  it('rejects non-text frames', () => {
    expect(parseMessage(new ArrayBuffer(4))).toBeNull();
  });
});
