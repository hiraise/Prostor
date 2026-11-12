import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { clock } from './clock.svelte.ts';

describe('clock', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
    vi.setSystemTime(Date.UTC(2026, 9, 21, 12, 0, 30));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('runs one timer for all readers, ticks at full minutes and stops when unused', () => {
    expect(vi.getTimerCount()).toBe(0);
    const seen: number[][] = [[], []];
    const stop = $effect.root(() => {
      $effect(() => {
        seen[0]?.push(clock.now);
      });
      $effect(() => {
        seen[1]?.push(clock.now);
      });
    });
    flushSync();
    expect(vi.getTimerCount()).toBe(1);

    // The first tick comes at the next full minute, then every minute.
    vi.advanceTimersByTime(29_999);
    flushSync();
    expect(seen[0]).toHaveLength(1);
    vi.advanceTimersByTime(1);
    flushSync();
    expect(seen[0]).toEqual([Date.UTC(2026, 9, 21, 12, 0, 30), Date.UTC(2026, 9, 21, 12, 1)]);
    vi.advanceTimersByTime(60_000);
    flushSync();
    expect(seen[1]).toHaveLength(3);

    stop();
    flushSync();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('reads the time without starting a timer outside reactive code', () => {
    expect(clock.now).toBe(Date.UTC(2026, 9, 21, 12, 0, 30));
    expect(vi.getTimerCount()).toBe(0);
  });
});
