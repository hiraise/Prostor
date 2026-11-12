// Keeps an open tab in step with instance-wide changes (ADM-002, ACC-049): the server's `instance`
// topic tells that settings or permissions changed, and the tab loads them again.
import { invalidate } from '$app/navigation';

import type { LiveConnection, Timers } from '#lib/live/connection.ts';
import { BOOTSTRAP_DEPENDENCY } from '#lib/api/system.ts';
import { refreshAccess } from '#lib/session/index.ts';
import { liveConnection } from '#lib/store/index.ts';

export const INSTANCE_TOPIC = 'instance';

/** Access reloads are spread over this long, so thousands of open tabs do not ask at once. */
export const ACCESS_SPREAD_MS = 2_000;

export interface InstanceFollowerOptions {
  connection?: Pick<LiveConnection, 'subscribe'>;
  /** Loads the configuration again. */
  reloadBootstrap?: () => Promise<unknown>;
  /** Loads the person's access map again. */
  reloadAccess?: () => Promise<unknown>;
  random?: () => number;
  timers?: Timers;
}

const globalTimers: Timers = {
  set: (callback, ms) => setTimeout(callback, ms),
  clear: (handle) => {
    clearTimeout(handle as ReturnType<typeof setTimeout>);
  },
};

function quietly(load: () => Promise<unknown>): void {
  // A failed reload keeps what the tab has; the next notice or page load tries again.
  load().catch(() => undefined);
}

/** Follows the `instance` topic until the returned function is called. */
export function followInstance(options: InstanceFollowerOptions = {}): () => void {
  const connection = options.connection ?? liveConnection();
  const reloadBootstrap = options.reloadBootstrap ?? (() => invalidate(BOOTSTRAP_DEPENDENCY));
  const reloadAccess = options.reloadAccess ?? refreshAccess;
  const random = options.random ?? Math.random;
  const timers = options.timers ?? globalTimers;

  let accessTimer: unknown = null;
  let subscribedBefore = false;

  const settings = (): void => {
    quietly(reloadBootstrap);
  };
  const access = (): void => {
    // One pending reload covers every notice that arrives before it runs.
    if (accessTimer !== null) return;
    accessTimer = timers.set(() => {
      accessTimer = null;
      quietly(reloadAccess);
    }, random() * ACCESS_SPREAD_MS);
  };

  const stop = connection.subscribe(INSTANCE_TOPIC, {
    event: (event) => {
      if (event.entity.kind === 'settings') settings();
      else if (event.entity.kind === 'access') access();
    },
    subscribed: () => {
      // Notices are never replayed: after a reconnect anything may have changed meanwhile.
      if (subscribedBefore) {
        settings();
        access();
      }
      subscribedBefore = true;
    },
    resync: () => {
      settings();
      access();
    },
  });

  return () => {
    stop();
    if (accessTimer !== null) timers.clear(accessTimer);
    accessTimer = null;
  };
}
