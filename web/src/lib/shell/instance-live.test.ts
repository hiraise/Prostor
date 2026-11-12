import { describe, expect, it, vi } from 'vitest';

import type { LiveEvent } from '#lib/api/schema.gen.ts';
import type { TopicHandlers, Timers } from '#lib/live/connection.ts';

import { ACCESS_SPREAD_MS, INSTANCE_TOPIC, followInstance } from './instance-live.ts';

vi.mock('$app/navigation', () => ({ invalidate: vi.fn(() => Promise.resolve()) }));

/** A connection with one topic the test drives by hand. */
class FakeConnection {
  readonly handlers = new Map<string, TopicHandlers>();
  unsubscribed = 0;

  subscribe(topic: string, handlers: TopicHandlers): () => void {
    this.handlers.set(topic, handlers);
    return () => {
      this.unsubscribed += 1;
      this.handlers.delete(topic);
    };
  }

  get instance(): TopicHandlers {
    const handlers = this.handlers.get(INSTANCE_TOPIC);
    if (!handlers) throw new Error('not subscribed to instance');
    return handlers;
  }
}

class FakeTimers implements Timers {
  readonly pending = new Map<number, { callback: () => void; ms: number }>();
  #next = 0;

  set(callback: () => void, ms: number): unknown {
    this.#next += 1;
    this.pending.set(this.#next, { callback, ms });
    return this.#next;
  }

  clear(handle: unknown): void {
    this.pending.delete(handle as number);
  }

  runAll(): void {
    const due = [...this.pending.values()];
    this.pending.clear();
    for (const { callback } of due) callback();
  }
}

function notice(kind: 'settings' | 'access'): LiveEvent {
  return {
    id: 7,
    topics: [INSTANCE_TOPIC],
    entity: { kind, id: 0, version: 0 },
    change: 'updated',
    patch: null,
  };
}

function setup(random = 0.5) {
  const connection = new FakeConnection();
  const timers = new FakeTimers();
  const reloadBootstrap = vi.fn(() => Promise.resolve());
  const reloadAccess = vi.fn(() => Promise.resolve());
  const stop = followInstance({
    connection,
    reloadBootstrap,
    reloadAccess,
    random: () => random,
    timers,
  });
  return { connection, timers, reloadBootstrap, reloadAccess, stop };
}

describe('followInstance', () => {
  it('adm_002_settings_notice_reloads_the_configuration', () => {
    const { connection, reloadBootstrap, reloadAccess, timers } = setup();
    connection.instance.event?.(notice('settings'));
    expect(reloadBootstrap).toHaveBeenCalledOnce();
    expect(timers.pending.size).toBe(0);
    expect(reloadAccess).not.toHaveBeenCalled();
  });

  it('acc_049_access_notice_reloads_access_after_a_random_delay', () => {
    const { connection, reloadBootstrap, reloadAccess, timers } = setup(0.25);
    connection.instance.event?.(notice('access'));
    connection.instance.event?.(notice('access'));
    expect(reloadAccess).not.toHaveBeenCalled();
    // Two notices before the reload: one reload.
    expect([...timers.pending.values()].map((timer) => timer.ms)).toEqual([
      0.25 * ACCESS_SPREAD_MS,
    ]);
    timers.runAll();
    expect(reloadAccess).toHaveBeenCalledOnce();
    expect(reloadBootstrap).not.toHaveBeenCalled();
    connection.instance.event?.(notice('access'));
    timers.runAll();
    expect(reloadAccess).toHaveBeenCalledTimes(2);
  });

  it('live_instance_reloads_both_after_a_reconnect', () => {
    const { connection, reloadBootstrap, reloadAccess, timers } = setup();
    connection.instance.subscribed?.();
    timers.runAll();
    expect(reloadBootstrap).not.toHaveBeenCalled();
    expect(reloadAccess).not.toHaveBeenCalled();
    connection.instance.subscribed?.();
    timers.runAll();
    expect(reloadBootstrap).toHaveBeenCalledOnce();
    expect(reloadAccess).toHaveBeenCalledOnce();
  });

  it('live_instance_ignores_other_kinds', () => {
    const { connection, reloadBootstrap, timers } = setup();
    connection.instance.event?.({
      ...notice('settings'),
      entity: { kind: 'issue', id: 1, version: 2 },
    });
    expect(reloadBootstrap).not.toHaveBeenCalled();
    expect(timers.pending.size).toBe(0);
  });

  it('live_instance_failed_reload_is_swallowed', async () => {
    const { connection, reloadBootstrap } = setup();
    reloadBootstrap.mockImplementationOnce(() => Promise.reject(new Error('offline')));
    connection.instance.event?.(notice('settings'));
    await Promise.resolve();
    expect(reloadBootstrap).toHaveBeenCalledOnce();
  });

  it('live_instance_stop_unsubscribes_and_drops_the_pending_reload', () => {
    const { connection, timers, reloadAccess, stop } = setup();
    connection.instance.event?.(notice('access'));
    stop();
    expect(connection.unsubscribed).toBe(1);
    expect(timers.pending.size).toBe(0);
    timers.runAll();
    expect(reloadAccess).not.toHaveBeenCalled();
  });
});
