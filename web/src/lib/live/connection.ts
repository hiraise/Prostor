// The live connection (D-007, architecture.md §5): one WebSocket per tab, shared by every view.
// Views subscribe to topics; the connection opens with the first topic, re-subscribes after a
// reconnect asking the server to replay what was missed, and closes when no topic is left.
import type {
  LiveClientMessage,
  LiveEntity,
  LiveEvent,
  LiveServerMessage,
} from '#lib/api/schema.gen.ts';

import { API_BASE } from '#lib/api/client.ts';
import { Cell } from '#lib/store/reactive.svelte.ts';

/** Codes the server closes a socket with; the same numbers as `close` in `crates/api/src/live.rs`. */
export const CloseCode = {
  /** The server stops: reconnect. */
  RESTART: 1012,
  /** The server refused a message; a correct client never gets it. */
  POLICY: 1008,
  /** The tab read too slowly: reconnect and replay. */
  TOO_SLOW: 4008,
  /** The session is over: do not reconnect. */
  SESSION_ENDED: 4401,
} as const;

/** The close code of a socket that was lost without one (network, failed open). */
export const ABNORMAL_CLOSE = 1006;

/** Where the connection stands; the shell shows it (UI-037). */
export type ConnectionStatus =
  /** No topics, no socket. */
  | 'idle'
  /** Opening the first socket. */
  | 'connecting'
  /** Connected; events arrive. */
  | 'open'
  /** The socket was lost; waiting or trying to connect again. */
  | 'reconnecting'
  /** The browser reports no network; trying again when it is back. */
  | 'offline';

/** What a subscriber hears about its topic. Every callback is optional. */
export interface TopicHandlers {
  /** A change of an entity in this topic. */
  event?: (event: LiveEvent) => void;
  /**
   * The topic is active: later changes are delivered. Called again after every reconnect whose
   * missed events were replayed.
   */
  subscribed?: () => void;
  /** Missed events cannot be replayed: reload what is shown for this topic. */
  resync?: () => void;
  /** The server refused the topic (unknown, or not visible to this user). */
  denied?: () => void;
}

/** A socket as the connection uses it; `createSocket` wires it to these callbacks. */
export interface LiveSocket {
  send(data: string): void;
  close(): void;
}

export interface LiveSocketCallbacks {
  open(): void;
  message(data: unknown): void;
  /** The socket closed or failed to open, with its close code; called at most once. */
  close(code: number): void;
}

/** Browser state the connection reacts to. */
export interface LiveEnvironment {
  /** Whether the browser believes it has a network. */
  online(): boolean;
  /** Listens for network and tab visibility changes; returns a function that stops listening. */
  listen(listener: (change: 'online' | 'offline' | 'visible' | 'hidden') => void): () => void;
}

export interface Timers {
  set(callback: () => void, ms: number): unknown;
  clear(handle: unknown): void;
}

export interface LiveConnectionOptions {
  /** WebSocket address; by default `/api/v1/live` on the page's host. */
  url?: () => string;
  createSocket?: (url: string, callbacks: LiveSocketCallbacks) => LiveSocket;
  environment?: LiveEnvironment;
  timers?: Timers;
  /** Random number in [0, 1) for reconnect jitter. */
  random?: () => number;
  /** First reconnect delay; doubles with every failed attempt. */
  retryInitialMs?: number;
  retryMaxMs?: number;
  /** An open socket that lasts this long resets the reconnect delay. */
  stableAfterMs?: number;
  pingEveryMs?: number;
  /** No `pong` within this time means the connection is dead. */
  pongTimeoutMs?: number;
  /** Keep the socket this long after the last topic is gone, in case a new view subscribes. */
  idleCloseMs?: number;
  /** A tab hidden at least this long checks its connection when shown again. */
  hiddenLongMs?: number;
  /** The server ended the connection because the session is over; the connection is idle now. */
  onSessionEnded?: () => void;
  /** Called when a subscriber's callback throws; by default the error is reported asynchronously. */
  onError?: (error: unknown) => void;
}

type TimerName = 'retry' | 'stable' | 'ping' | 'pong' | 'idle';

interface Topic {
  readonly listeners: Set<TopicHandlers>;
}

const LIVE_PATH = `${API_BASE}/live`;

function defaultUrl(): string {
  const { protocol, host } = globalThis.location;
  return `${protocol === 'https:' ? 'wss:' : 'ws:'}//${host}${LIVE_PATH}`;
}

function browserSocket(url: string, callbacks: LiveSocketCallbacks): LiveSocket {
  const socket = new WebSocket(url);
  let closed = false;
  socket.onopen = () => {
    callbacks.open();
  };
  socket.onmessage = (event) => {
    callbacks.message(event.data);
  };
  // `error` is always followed by `close`, so only `close` is reported.
  socket.onclose = (event) => {
    if (closed) return;
    closed = true;
    callbacks.close(event.code);
  };
  return {
    send: (data) => {
      socket.send(data);
    },
    close: () => {
      socket.close();
    },
  };
}

const browserEnvironment: LiveEnvironment = {
  online: () => typeof navigator === 'undefined' || navigator.onLine,
  listen(listener) {
    if (typeof window === 'undefined') return () => undefined;
    const online = (): void => {
      listener('online');
    };
    const offline = (): void => {
      listener('offline');
    };
    const visibility = (): void => {
      listener(document.visibilityState === 'visible' ? 'visible' : 'hidden');
    };
    window.addEventListener('online', online);
    window.addEventListener('offline', offline);
    document.addEventListener('visibilitychange', visibility);
    return () => {
      window.removeEventListener('online', online);
      window.removeEventListener('offline', offline);
      document.removeEventListener('visibilitychange', visibility);
    };
  },
};

// Arrow functions look the globals up at call time, so test fake timers apply.
const globalTimers: Timers = {
  set: (callback, ms) => setTimeout(callback, ms),
  clear: (handle) => {
    clearTimeout(handle as ReturnType<typeof setTimeout>);
  },
};

function reportLater(error: unknown): void {
  queueMicrotask(() => {
    throw error;
  });
}

export class LiveConnection {
  readonly #status = new Cell<ConnectionStatus>('idle');

  readonly #topics = new Map<string, Topic>();
  readonly #url: () => string;
  readonly #createSocket: (url: string, callbacks: LiveSocketCallbacks) => LiveSocket;
  readonly #environment: LiveEnvironment;
  readonly #timers: Timers;
  readonly #random: () => number;
  readonly #onError: (error: unknown) => void;
  readonly #onSessionEnded: () => void;
  readonly #retryInitialMs: number;
  readonly #retryMaxMs: number;
  readonly #stableAfterMs: number;
  readonly #pingEveryMs: number;
  readonly #pongTimeoutMs: number;
  readonly #idleCloseMs: number;
  readonly #hiddenLongMs: number;

  #socket: LiveSocket | null = null;
  #socketOpen = false;
  /** Whether a socket has been open before: later sockets resume with `since`. */
  #wasOpen = false;
  /** Position in the event sequence: the larger of the last event ID and the last `head`. */
  #since: number | null = null;
  /** Failed attempts since the last stable connection. */
  #attempts = 0;
  #hiddenAt: number | null = null;
  #stopListening: (() => void) | null = null;

  readonly #timerHandles = new Map<TimerName, unknown>();

  constructor(options: LiveConnectionOptions = {}) {
    this.#url = options.url ?? defaultUrl;
    this.#createSocket = options.createSocket ?? browserSocket;
    this.#environment = options.environment ?? browserEnvironment;
    this.#timers = options.timers ?? globalTimers;
    this.#random = options.random ?? Math.random;
    this.#onError = options.onError ?? reportLater;
    this.#onSessionEnded = options.onSessionEnded ?? (() => undefined);
    this.#retryInitialMs = options.retryInitialMs ?? 500;
    this.#retryMaxMs = options.retryMaxMs ?? 30_000;
    this.#stableAfterMs = options.stableAfterMs ?? 10_000;
    this.#pingEveryMs = options.pingEveryMs ?? 25_000;
    this.#pongTimeoutMs = options.pongTimeoutMs ?? 10_000;
    this.#idleCloseMs = options.idleCloseMs ?? 10_000;
    this.#hiddenLongMs = options.hiddenLongMs ?? 60_000;
  }

  /** Where the connection stands; reading it inside a component tracks changes. */
  get status(): ConnectionStatus {
    return this.#status.value;
  }

  /** Topics with at least one subscriber. */
  get topics(): string[] {
    return [...this.#topics.keys()];
  }

  /**
   * Starts listening to a topic and returns the function that stops. Several subscribers of one
   * topic share one server subscription.
   */
  subscribe(topic: string, handlers: TopicHandlers): () => void {
    // Each call gets its own entry, so subscribing the same handlers twice needs two unsubscribes.
    const listener: TopicHandlers = { ...handlers };
    let entry = this.#topics.get(topic);
    if (!entry) {
      entry = { listeners: new Set() };
      this.#topics.set(topic, entry);
      if (this.#socketOpen) this.#send({ type: 'subscribe', topics: [topic], since: null });
    }
    entry.listeners.add(listener);
    this.#clearTimer('idle');
    if (this.#status.value === 'idle') this.#connect();

    let subscribed = true;
    return () => {
      if (!subscribed) return;
      subscribed = false;
      this.#unsubscribe(topic, listener);
    };
  }

  /** Closes the socket and drops every subscription. */
  close(): void {
    this.#topics.clear();
    this.#goIdle();
  }

  #unsubscribe(topic: string, listener: TopicHandlers): void {
    const entry = this.#topics.get(topic);
    if (!entry) return;
    entry.listeners.delete(listener);
    if (entry.listeners.size > 0) return;
    this.#topics.delete(topic);
    if (this.#socketOpen) this.#send({ type: 'unsubscribe', topics: [topic] });
    if (this.#topics.size > 0) return;
    if (this.#socket) {
      this.#setTimer(
        'idle',
        () => {
          this.#goIdle();
        },
        this.#idleCloseMs,
      );
    } else {
      // Waiting to reconnect: nobody needs the connection any more.
      this.#goIdle();
    }
  }

  #connect(): void {
    this.#clearTimer('retry');
    this.#stopListening ??= this.#environment.listen((change) => {
      this.#onEnvironment(change);
    });
    this.#status.value = this.#wasOpen || this.#attempts > 0 ? 'reconnecting' : 'connecting';
    let current: LiveSocket | null = null;
    const isCurrent = (): boolean => current !== null && this.#socket === current;
    current = this.#createSocket(this.#url(), {
      open: () => {
        if (isCurrent()) this.#opened();
      },
      message: (data) => {
        if (isCurrent()) this.#received(data);
      },
      close: (code) => {
        if (isCurrent()) this.#closed(code);
      },
    });
    this.#socket = current;
  }

  #opened(): void {
    this.#socketOpen = true;
    this.#status.value = 'open';
    const topics = this.topics;
    if (topics.length > 0) {
      this.#send({
        type: 'subscribe',
        topics,
        since: this.#wasOpen ? this.#since : null,
      });
    }
    this.#wasOpen = true;
    this.#setTimer(
      'stable',
      () => {
        this.#attempts = 0;
      },
      this.#stableAfterMs,
    );
    this.#schedulePing();
  }

  /** The socket closed: every code but the end of the session reconnects. */
  #closed(code: number): void {
    if (code === CloseCode.SESSION_ENDED) {
      this.#topics.clear();
      this.#goIdle();
      try {
        this.#onSessionEnded();
      } catch (error) {
        this.#onError(error);
      }
      return;
    }
    if (code === CloseCode.POLICY) {
      console.warn('live: the server refused a message of this client');
    }
    this.#lost();
  }

  /** The socket closed or stopped answering: try again after a growing delay. */
  #lost(): void {
    this.#dropSocket();
    if (this.#topics.size === 0) {
      this.#goIdle();
      return;
    }
    this.#attempts += 1;
    const ceiling = Math.min(this.#retryMaxMs, this.#retryInitialMs * 2 ** (this.#attempts - 1));
    // Half fixed, half random: clients that lost the same server do not return at once.
    const delay = ceiling / 2 + (this.#random() * ceiling) / 2;
    this.#status.value = this.#environment.online() ? 'reconnecting' : 'offline';
    this.#setTimer(
      'retry',
      () => {
        this.#connect();
      },
      delay,
    );
  }

  #goIdle(): void {
    this.#dropSocket();
    this.#clearTimer('retry');
    this.#clearTimer('idle');
    this.#stopListening?.();
    this.#stopListening = null;
    this.#attempts = 0;
    this.#status.value = 'idle';
  }

  #dropSocket(): void {
    const socket = this.#socket;
    this.#socket = null;
    this.#socketOpen = false;
    this.#clearTimer('stable');
    this.#clearTimer('ping');
    this.#clearTimer('pong');
    if (socket) {
      try {
        socket.close();
      } catch {
        // Already closed.
      }
    }
  }

  #onEnvironment(change: 'online' | 'offline' | 'visible' | 'hidden'): void {
    const waiting = this.#socket === null && this.#topics.size > 0;
    switch (change) {
      case 'online':
        if (waiting) this.#connect();
        break;
      case 'offline':
        if (waiting) this.#status.value = 'offline';
        break;
      case 'hidden':
        this.#hiddenAt = Date.now();
        break;
      case 'visible': {
        const hiddenLong =
          this.#hiddenAt !== null && Date.now() - this.#hiddenAt >= this.#hiddenLongMs;
        this.#hiddenAt = null;
        if (!hiddenLong) break;
        // After a long sleep the socket may be dead without knowing it.
        if (waiting) this.#connect();
        else if (this.#socketOpen) this.#ping();
        break;
      }
    }
  }

  #schedulePing(): void {
    this.#setTimer(
      'ping',
      () => {
        this.#ping();
      },
      this.#pingEveryMs,
    );
  }

  #ping(): void {
    this.#send({ type: 'ping' });
    this.#clearTimer('ping');
    if (!this.#timerHandles.has('pong')) {
      this.#setTimer(
        'pong',
        () => {
          this.#lost();
        },
        this.#pongTimeoutMs,
      );
    }
  }

  #received(data: unknown): void {
    const message = parseMessage(data);
    if (!message) return;
    switch (message.type) {
      case 'event':
        this.#since = Math.max(this.#since ?? message.id, message.id);
        this.#notify(message.topics, (listener) => listener.event?.(message));
        break;
      case 'subscribed':
        this.#since = Math.max(this.#since ?? message.head, message.head);
        this.#notify(message.topics, (listener) => listener.subscribed?.());
        break;
      case 'denied':
        this.#notify(message.topics, (listener) => listener.denied?.());
        break;
      case 'resync':
        this.#notify(message.topics, (listener) => listener.resync?.());
        break;
      case 'pong':
        this.#clearTimer('pong');
        this.#schedulePing();
        break;
    }
  }

  /** Calls each listener of these topics once, even if it listens to several of them. */
  #notify(topics: readonly string[], call: (listener: TopicHandlers) => void): void {
    const listeners = new Set<TopicHandlers>();
    for (const topic of topics) {
      for (const listener of this.#topics.get(topic)?.listeners ?? []) listeners.add(listener);
    }
    for (const listener of listeners) {
      try {
        call(listener);
      } catch (error) {
        this.#onError(error);
      }
    }
  }

  #send(message: LiveClientMessage): void {
    if (!this.#socket || !this.#socketOpen) return;
    try {
      this.#socket.send(JSON.stringify(message));
    } catch {
      // The socket is closing; its `close` callback reconnects.
    }
  }

  #setTimer(name: TimerName, callback: () => void, ms: number): void {
    this.#clearTimer(name);
    const handle = this.#timers.set(() => {
      this.#timerHandles.delete(name);
      callback();
    }, ms);
    this.#timerHandles.set(name, handle);
  }

  #clearTimer(name: TimerName): void {
    if (!this.#timerHandles.has(name)) return;
    this.#timers.clear(this.#timerHandles.get(name));
    this.#timerHandles.delete(name);
  }
}

// Incoming messages are checked just enough that handlers can trust their shape; anything else
// (a newer server's message type, a broken frame) is ignored.

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === 'string');
}

function parseEntity(value: unknown): LiveEntity | null {
  if (!isObject(value)) return null;
  const { kind, id, version } = value;
  return typeof kind === 'string' && isInteger(id) && isInteger(version)
    ? { kind, id, version }
    : null;
}

export function parseMessage(data: unknown): LiveServerMessage | null {
  if (typeof data !== 'string') return null;
  let value: unknown;
  try {
    value = JSON.parse(data);
  } catch {
    return null;
  }
  if (!isObject(value)) return null;
  switch (value.type) {
    case 'pong':
      return { type: 'pong' };
    case 'subscribed':
      return isStringArray(value.topics) && isInteger(value.head)
        ? { type: 'subscribed', topics: value.topics, head: value.head }
        : null;
    case 'denied':
    case 'resync':
      return isStringArray(value.topics) ? { type: value.type, topics: value.topics } : null;
    case 'event': {
      const { id, topics, change, patch } = value;
      const entity = parseEntity(value.entity);
      const valid =
        isInteger(id) &&
        isStringArray(topics) &&
        entity !== null &&
        (change === 'created' || change === 'updated' || change === 'deleted') &&
        (patch === null || patch === undefined || isObject(patch));
      return valid ? { type: 'event', id, topics, entity, change, patch: patch ?? null } : null;
    }
    default:
      return null;
  }
}
