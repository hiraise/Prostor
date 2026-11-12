// List queries (architecture.md §15): a result keeps entity IDs and the cursor, never copies of
// entities; the entities live in their table, retained by the query while it exists.
import { createSubscriber } from 'svelte/reactivity';

import type { LiveEvent } from '#lib/api/schema.gen.ts';
import type { TopicHandlers } from '#lib/live/connection.ts';

import type { Entity, EntityTable, Form } from './entities.ts';
import { reactive } from './reactive.svelte.ts';

/** One page of a list endpoint (`crates/api/src/page.rs`). */
export interface Page<T> {
  items: T[];
  next_cursor: string | null;
}

/** How long live events of shown rows are gathered before one refetch (plan 1.7 §3.10). */
export const LIVE_BATCH_MS = 200;

/** What a list follows its topic with (`LiveConnection`). */
export interface LiveSource {
  subscribe(topic: string, handlers: TopicHandlers): () => void;
}

/** How to load a list. */
export interface ListSpec<T extends Entity> {
  /** Identifies the result: equal keys (compared as JSON) share one cached list. */
  key: readonly unknown[];
  /** Loads the page after `cursor` (`null`: the first page). */
  fetch: (cursor: string | null, signal: AbortSignal) => Promise<Page<T>>;
  /** Form of the items the endpoint returns; default `short`. */
  form?: Form;
  /** How long a loaded result counts as fresh; older results show at once and reload quietly. */
  staleTimeMs?: number;
}

interface ListState {
  ids: number[];
  nextCursor: string | null;
  /** The first page is loading (first load or revalidation). */
  refreshing: boolean;
  /** A next page is loading. */
  loadingMore: boolean;
  /** Error of the last load; the IDs from before stay. */
  error: unknown;
  /** A live event may have changed which rows belong here or their order; cleared by a reload. */
  stale: boolean;
}

/** Stable text of a key: object keys sorted, so `{a, b}` and `{b, a}` are the same list. */
export function keyText(key: readonly unknown[]): string {
  return JSON.stringify(key, (_name, value: unknown) =>
    typeof value === 'object' && value !== null && !Array.isArray(value)
      ? Object.fromEntries(
          Object.entries(value as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : 1)),
        )
      : value,
  );
}

/** One cached list result. */
export class ListQuery<T extends Entity> {
  readonly key: string;
  readonly state: ListState = reactive({
    ids: [],
    nextCursor: null,
    refreshing: false,
    loadingMore: false,
    error: null,
    stale: false,
  });
  /** When the first page was last loaded; `-Infinity` before that. */
  fetchedAt = -Infinity;
  /** Settles when the first load does; the same promise for every caller of this key. */
  ready: Promise<ListHandle<T>> | null = null;
  readonly handle: ListHandle<T>;

  readonly #spec: ListSpec<T>;
  readonly #table: EntityTable<T>;
  readonly #now: () => number;
  readonly #kind: string;
  readonly #refetch: (ids: number[]) => Promise<void>;
  /** Shown rows a live event changed, refetched together after `LIVE_BATCH_MS`. */
  readonly #changed = new Set<number>();
  #liveTimer: ReturnType<typeof setTimeout> | null = null;
  /** Row retentions by ID. */
  readonly #rows = new Map<number, () => void>();
  readonly #abort = new AbortController();
  #first: Promise<void> | null = null;
  #more: Promise<void> | null = null;
  /** Increases with every first-page load, so a next page of an older result is dropped. */
  #generation = 0;

  constructor(
    key: string,
    spec: ListSpec<T>,
    table: EntityTable<T>,
    retain: () => () => void,
    now: () => number,
    live: { kind: string; refetch: (ids: number[]) => Promise<void> },
  ) {
    this.key = key;
    this.#spec = spec;
    this.#table = table;
    this.#now = now;
    this.#kind = live.kind;
    this.#refetch = live.refetch;
    this.handle = new ListHandle(this, table, retain);
  }

  get staleTimeMs(): number | undefined {
    return this.#spec.staleTimeMs;
  }

  /** Loads the first page again; concurrent calls share one request. */
  load(): Promise<void> {
    this.#first ??= this.#loadFirst().finally(() => {
      this.#first = null;
    });
    return this.#first;
  }

  /** Loads the next page and appends its IDs; concurrent calls share one request. */
  loadMore(): Promise<void> {
    if (this.state.nextCursor === null || this.#first) return this.#first ?? Promise.resolve();
    this.#more ??= this.#loadNext(this.state.nextCursor).finally(() => {
      this.#more = null;
    });
    return this.#more;
  }

  /** Reloads quietly: a failure is kept in `error`, the old IDs stay. */
  revalidate(): void {
    this.load().catch(() => undefined);
  }

  /**
   * Follows the list's live topic until the returned function is called. Shown rows that change
   * are refetched together; any other change of this kind only marks the list stale, so rows never
   * jump under the pointer: membership, order and count change on a reload.
   */
  follow(source: LiveSource, topic: string): () => void {
    const stop = source.subscribe(topic, {
      event: (event) => {
        this.#live(event);
      },
      // Missed events cannot be replayed: refresh what is shown and offer the reload.
      resync: () => {
        this.state.stale = true;
        for (const id of this.state.ids) this.#queueRefetch(id);
      },
    });
    return () => {
      stop();
      this.#stopLive();
    };
  }

  #live(event: LiveEvent): void {
    const { kind, id, version } = event.entity;
    if (kind !== this.#kind) return;
    if (!this.#rows.has(id)) {
      this.state.stale = true;
      return;
    }
    // A deleted row stays listed until the reload; the refetch drops it from the table.
    if (event.change === 'deleted') this.state.stale = true;
    else if ((this.#table.get(id)?.version ?? -1) >= version) return;
    this.#queueRefetch(id);
  }

  #queueRefetch(id: number): void {
    this.#changed.add(id);
    this.#liveTimer ??= setTimeout(() => {
      this.#liveTimer = null;
      const ids = [...this.#changed].sort((a, b) => a - b);
      this.#changed.clear();
      this.#refetch(ids).catch((error: unknown) => {
        // The rows keep what they showed; the next event tries again.
        console.error('live: list refetch failed', error);
      });
    }, LIVE_BATCH_MS);
  }

  #stopLive(): void {
    if (this.#liveTimer !== null) clearTimeout(this.#liveTimer);
    this.#liveTimer = null;
    this.#changed.clear();
  }

  /** Stops loading and releases the rows. */
  dispose(): void {
    this.#stopLive();
    this.#abort.abort();
    for (const release of this.#rows.values()) release();
    this.#rows.clear();
  }

  async #loadFirst(): Promise<void> {
    const generation = ++this.#generation;
    this.state.refreshing = true;
    try {
      const page = await this.#spec.fetch(null, this.#abort.signal);
      if (this.#abort.signal.aborted) return;
      this.#table.upsertMany(page.items, this.#spec.form ?? 'short');
      this.#setIds(page.items.map((item) => item.id));
      this.state.nextCursor = page.next_cursor;
      this.state.error = null;
      this.state.stale = false;
      this.fetchedAt = this.#now();
    } catch (error) {
      if (generation === this.#generation) this.state.error = error;
      throw error;
    } finally {
      if (generation === this.#generation) this.state.refreshing = false;
    }
  }

  async #loadNext(cursor: string): Promise<void> {
    const generation = this.#generation;
    this.state.loadingMore = true;
    try {
      const page = await this.#spec.fetch(cursor, this.#abort.signal);
      // A newer first page replaced the list this cursor belongs to.
      if (generation !== this.#generation || this.#abort.signal.aborted) return;
      this.#table.upsertMany(page.items, this.#spec.form ?? 'short');
      const known = new Set(this.state.ids);
      const added = page.items.map((item) => item.id).filter((id) => !known.has(id));
      this.#setIds([...this.state.ids, ...added]);
      this.state.nextCursor = page.next_cursor;
      this.state.error = null;
    } catch (error) {
      if (generation === this.#generation) this.state.error = error;
      throw error;
    } finally {
      this.state.loadingMore = false;
    }
  }

  /**
   * Shows the listed rows in this order (an optimistic reorder or removal); IDs the list does not
   * hold are left out. A reload brings the server's order back.
   */
  arrange(ids: readonly number[]): void {
    const known = new Set(this.state.ids);
    this.#setIds(ids.filter((id) => known.has(id)));
  }

  /** Replaces the IDs, retaining new rows and releasing dropped ones. */
  #setIds(ids: number[]): void {
    const next = new Set(ids);
    for (const [id, release] of this.#rows) {
      if (!next.has(id)) {
        release();
        this.#rows.delete(id);
      }
    }
    for (const id of next) {
      if (!this.#rows.has(id)) this.#rows.set(id, this.#table.retain(id));
    }
    this.state.ids = ids;
  }
}

/**
 * What a view gets for a list. Reading any of its values inside a component retains the list
 * (and so its rows) while the component shows it.
 */
export class ListHandle<T extends Entity> {
  readonly #query: ListQuery<T>;
  readonly #table: EntityTable<T>;
  readonly #retain: () => () => void;
  readonly #subscribe: () => void;

  constructor(query: ListQuery<T>, table: EntityTable<T>, retain: () => () => void) {
    this.#query = query;
    this.#table = table;
    this.#retain = retain;
    this.#subscribe = createSubscriber(() => retain());
  }

  /** IDs in list order. */
  get ids(): readonly number[] {
    this.#subscribe();
    return this.#query.state.ids;
  }

  /** The rows in list order; for long lists render `ids` with a virtual list instead. */
  get items(): T[] {
    const rows: T[] = [];
    for (const id of this.ids) {
      const row = this.#table.get(id);
      if (row) rows.push(row);
    }
    return rows;
  }

  get hasMore(): boolean {
    this.#subscribe();
    return this.#query.state.nextCursor !== null;
  }

  /** The first page is (re)loading. */
  get refreshing(): boolean {
    this.#subscribe();
    return this.#query.state.refreshing;
  }

  get loadingMore(): boolean {
    this.#subscribe();
    return this.#query.state.loadingMore;
  }

  /** Error of the last load, shown next to the data that stayed. */
  get error(): unknown {
    this.#subscribe();
    return this.#query.state.error;
  }

  /** A live event may have changed the list's rows, order or count (the «Обновить» hint). */
  get stale(): boolean {
    this.#subscribe();
    return this.#query.state.stale;
  }

  loadMore(): Promise<void> {
    return this.#query.loadMore();
  }

  /** Follows the list's live topic (`query.follow`); returns the function that stops. */
  follow(source: LiveSource, topic: string): () => void {
    return this.#query.follow(source, topic);
  }

  /** Shows the listed rows in this order; see `ListQuery.arrange`. */
  arrange(ids: readonly number[]): void {
    this.#query.arrange(ids);
  }

  /** Reloads the first page; rejects when it fails. */
  refresh(): Promise<void> {
    return this.#query.load();
  }

  /** Keeps the list outside a component (inside one, reading it is enough). */
  retain(): () => void {
    return this.#retain();
  }
}
