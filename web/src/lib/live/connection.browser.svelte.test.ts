// The connection status is reactive, so the shell can show it (needs the client build of Svelte).
import { flushSync } from 'svelte';
import { expect, it } from 'vitest';

import { LiveConnection, type LiveSocketCallbacks } from './connection.ts';

it('wakes readers of the status when it changes', () => {
  let socket: LiveSocketCallbacks | null = null;
  const connection = new LiveConnection({
    url: () => 'ws://test',
    createSocket: (_url, callbacks) => {
      socket = callbacks;
      return { send: () => undefined, close: () => undefined };
    },
  });
  const seen: string[] = [];
  const stop = $effect.root(() => {
    $effect(() => {
      seen.push(connection.status);
    });
  });
  flushSync();

  connection.subscribe('inbox', {});
  flushSync();
  (socket as LiveSocketCallbacks | null)?.open();
  flushSync();
  connection.close();
  flushSync();
  expect(seen).toEqual(['idle', 'connecting', 'open', 'idle']);
  stop();
});
