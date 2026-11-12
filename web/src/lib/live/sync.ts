// Live events → entity store (architecture.md §5). A change with a usable patch is applied at
// once; otherwise the entity is fetched again, batched per kind so a bulk change of N entities
// costs one request. Only entities the store already keeps are fetched: nobody shows the others.
import type { LiveEvent } from '#lib/api/schema.gen.ts';
import type { Entity, EntityTable } from '#lib/store/entities.ts';

import type { LiveConnection, TopicHandlers } from './connection.ts';

export interface LiveSyncOptions {
  /** Where the entity tables are (`EntityStore`). */
  store: { find(kind: string): EntityTable<Entity> | undefined };
  /** Loads the current state of these entities into the store. */
  refetch: (kind: string, ids: number[]) => void | Promise<void>;
  /** How long to collect entities to fetch before fetching them together. */
  batchMs?: number;
  /** Called when a refetch fails; by default the error is logged. */
  onError?: (error: unknown) => void;
}

/** Topic callbacks a view adds to the store updates. */
export type FollowHandlers = Omit<TopicHandlers, 'event'> & {
  /** Called after the store has handled the event, for views that track more than entities. */
  event?: (event: LiveEvent) => void;
};

export class LiveSync {
  readonly #store: LiveSyncOptions['store'];
  readonly #refetch: LiveSyncOptions['refetch'];
  readonly #batchMs: number;
  readonly #onError: (error: unknown) => void;
  /** Entities to fetch, by kind. */
  readonly #pending = new Map<string, Set<number>>();
  #flushTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(options: LiveSyncOptions) {
    this.#store = options.store;
    this.#refetch = options.refetch;
    this.#batchMs = options.batchMs ?? 20;
    this.#onError =
      options.onError ??
      ((error) => {
        console.error('live: refetch failed', error);
      });
  }

  /** Brings the store up to date with one event. */
  handle(event: LiveEvent): void {
    const { kind, id, version } = event.entity;
    const table = this.#store.find(kind);
    if (!table) return;

    if (event.change === 'deleted') {
      this.#pending.get(kind)?.delete(id);
      table.remove(id);
      return;
    }

    const stored = table.get(id);
    if (!stored) {
      // A created entity arrives whole; keep it, since a view of its topic may show it.
      if (event.change === 'created' && event.patch) {
        table.upsert({ ...event.patch, id, version }, 'full');
      }
      return;
    }
    if (stored.version >= version) return;
    if (event.patch && table.applyPatch(id, version, event.patch) === 'applied') return;
    this.#queue(kind, id);
  }

  /**
   * Subscribes to a topic whose events update the store; returns the function that stops.
   * `resync` is the view's to handle: only it knows what it shows.
   */
  follow(
    connection: Pick<LiveConnection, 'subscribe'>,
    topic: string,
    handlers: FollowHandlers = {},
  ): () => void {
    return connection.subscribe(topic, {
      ...handlers,
      event: (event) => {
        this.handle(event);
        handlers.event?.(event);
      },
    });
  }

  /** Fetches queued entities now instead of waiting for the batch window. */
  flush(): void {
    if (this.#flushTimer !== null) {
      clearTimeout(this.#flushTimer);
      this.#flushTimer = null;
    }
    const batches = [...this.#pending].filter(([, ids]) => ids.size > 0);
    this.#pending.clear();
    for (const [kind, ids] of batches) {
      try {
        Promise.resolve(
          this.#refetch(
            kind,
            [...ids].sort((a, b) => a - b),
          ),
        ).catch(this.#onError);
      } catch (error) {
        this.#onError(error);
      }
    }
  }

  /** Drops queued fetches. */
  dispose(): void {
    if (this.#flushTimer !== null) clearTimeout(this.#flushTimer);
    this.#flushTimer = null;
    this.#pending.clear();
  }

  #queue(kind: string, id: number): void {
    let ids = this.#pending.get(kind);
    if (!ids) {
      ids = new Set();
      this.#pending.set(kind, ids);
    }
    ids.add(id);
    this.#flushTimer ??= setTimeout(() => {
      this.#flushTimer = null;
      this.flush();
    }, this.#batchMs);
  }
}
