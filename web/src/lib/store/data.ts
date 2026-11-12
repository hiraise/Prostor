// The client data layer (architecture.md §15, D-020): entity tables, list queries, loading with
// deduplication and stale-while-revalidate, eviction of what no view shows, and optimistic edits.
// Route `load` functions call `ensure…`, so a preload on hover fills the cache.
import { createSubscriber } from 'svelte/reactivity';

import { ApiError } from '#lib/api/client.ts';

import {
  EntityStore,
  covers,
  sameJson,
  type Entity,
  type EntityTable,
  type Form,
  type KindMap,
} from './entities.ts';
import { ListQuery, keyText, type ListHandle, type ListSpec } from './queries.ts';
import { snapshot } from './reactive.svelte.ts';
import { Retention } from './retention.ts';

/** How the store loads entities of a kind. */
export interface KindLoader<T extends Entity> {
  /**
   * Loads these entities in the given form. IDs missing from the result are gone or no longer
   * visible: the store drops them.
   */
  load(ids: number[], form: Form): Promise<T[]>;
}

export interface DataStoreOptions {
  /** How long data no view retains stays in memory. */
  evictAfterMs?: number;
  /** How long loaded data counts as fresh unless a call says otherwise. */
  staleTimeMs?: number;
  now?: () => number;
}

export interface EnsureEntityOptions {
  /** The form the caller shows; `full` loads the full form when the row holds only the short one. */
  form?: Form;
  staleTimeMs?: number;
}

/** Fields an edit may change. */
export type Changes<T> = Partial<Omit<T, 'id' | 'version'>>;

/** What `update` sends: the changes and the version they were made on. */
export interface EditRequest<T> {
  changes: Changes<T>;
  version: number;
}

/**
 * Someone else changed the same fields first. The row shows the server's state; the UI offers
 * to apply `attempted` again or drop it.
 */
export class ConflictError<T extends Entity = Entity> extends Error {
  readonly kind: string;
  readonly id: number;
  /** The entity as the server has it now. */
  readonly server: T;
  /** The changes that were not applied. */
  readonly attempted: Changes<T>;

  constructor(kind: string, id: number, server: T, attempted: Changes<T>) {
    super(`${kind} ${String(id)} was changed by someone else`);
    this.name = 'ConflictError';
    this.kind = kind;
    this.id = id;
    this.server = server;
    this.attempted = attempted;
  }
}

/** The answer for an entity that does not exist or is not visible (architecture.md §8). */
function notFound(kind: string, id: number): ApiError {
  return new ApiError(404, { code: 'not_found', message: `${kind} ${String(id)} not found` });
}

/**
 * What a view gets for one entity: a reference into the store, not a copy. Reading `current`
 * inside a component retains the row while the component shows it.
 */
export class EntityHandle<T extends Entity> {
  readonly id: number;
  readonly #table: EntityTable<T>;
  readonly #need: Form;
  readonly #subscribe: () => void;

  constructor(table: EntityTable<T>, id: number, need: Form) {
    this.id = id;
    this.#table = table;
    this.#need = need;
    this.#subscribe = createSubscriber(() => table.retain(id, need));
  }

  /** The row; `undefined` after the entity was deleted or became invisible. */
  get current(): T | undefined {
    this.#subscribe();
    return this.#table.get(this.id);
  }

  /** Keeps the row outside a component (inside one, reading `current` is enough). */
  retain(): () => void {
    return this.#table.retain(this.id, this.#need);
  }
}

interface EntityLoad {
  form: Form;
  ready: Promise<EntityHandle<Entity>>;
}

export class DataStore<K extends KindMap<K>> {
  readonly entities: EntityStore<K>;
  readonly #loaders = new Map<string, KindLoader<Entity>>();
  readonly #loads = new Map<string, EntityLoad>();
  readonly #queries = new Map<string, ListQuery<Entity>>();
  readonly #queryRetention: Retention<string>;
  readonly #staleTimeMs: number;
  readonly #now: () => number;

  constructor(options: DataStoreOptions = {}) {
    const evictAfterMs = options.evictAfterMs ?? 5 * 60_000;
    this.#staleTimeMs = options.staleTimeMs ?? 10_000;
    this.#now = options.now ?? Date.now;
    this.entities = new EntityStore<K>({
      evictAfterMs,
      now: this.#now,
      // A view still shows the full form: load it again.
      onDowngrade: (kind, id) => {
        this.#loadEntity(kind, id, 'full').catch(() => undefined);
      },
    });
    this.#queryRetention = new Retention(evictAfterMs, (key) => {
      this.#queries.get(key)?.dispose();
      this.#queries.delete(key);
    });
  }

  /** Tells the store how to load a kind; each feature defines its kinds once. */
  defineKind<N extends keyof K & string>(kind: N, loader: KindLoader<K[N]>): void {
    this.#loaders.set(kind, loader);
  }

  /**
   * The entity for a view. Loads it when the store lacks it (or lacks the form); otherwise
   * resolves at once and reloads quietly when stale. Calls for an entity already loading share
   * one promise.
   */
  ensureEntity<N extends keyof K & string>(
    kind: N,
    id: number,
    options: EnsureEntityOptions = {},
  ): Promise<EntityHandle<K[N]>> {
    const table = this.entities.table(kind);
    const form = options.form ?? 'short';
    const loading = this.#loads.get(`${kind}:${String(id)}`);
    if (loading && covers(loading.form, form)) {
      return loading.ready as unknown as Promise<EntityHandle<K[N]>>;
    }
    if (covers(table.form(id), form)) {
      const age = this.#now() - (table.fetchedAt(id) ?? -Infinity);
      if (age > (options.staleTimeMs ?? this.#staleTimeMs)) {
        this.#loadEntity(kind, id, table.form(id) ?? form).catch(() => undefined);
      }
      return Promise.resolve(new EntityHandle(table, id, form));
    }
    return this.#loadEntity(kind, id, form) as unknown as Promise<EntityHandle<K[N]>>;
  }

  /**
   * The list for a view. Loads the first page when there is no result for this key; otherwise
   * resolves at once and reloads quietly when stale. Every call with the same key gets the same
   * promise while the result is cached.
   */
  ensureList<N extends keyof K & string>(kind: N, spec: ListSpec<K[N]>): Promise<ListHandle<K[N]>> {
    const key = `${kind}:${keyText(spec.key)}`;
    const cached = this.#queries.get(key) as ListQuery<K[N]> | undefined;
    if (cached?.ready) {
      const age = this.#now() - cached.fetchedAt;
      const fresh = age <= (cached.staleTimeMs ?? this.#staleTimeMs);
      if (cached.fetchedAt > -Infinity && !fresh) cached.revalidate();
      return cached.ready;
    }

    const query = new ListQuery<K[N]>(
      key,
      spec,
      this.entities.table(kind),
      () => this.#queryRetention.retain(key),
      this.#now,
      { kind, refetch: (ids) => this.refetch(kind, ids) },
    );
    this.#queries.set(key, query);
    this.#queryRetention.track(key);
    query.ready = query.load().then(
      () => query.handle,
      (error: unknown) => {
        // Not cached: the next call tries again.
        if (this.#queries.get(key) === (query as unknown as ListQuery<Entity>)) {
          this.#queries.delete(key);
          this.#queryRetention.forget(key);
          query.dispose();
        }
        throw error;
      },
    );
    return query.ready;
  }

  /**
   * Stores entities a request answered along with something else (the issue page's comments, for
   * one), in the form they came in. They merge by version as a load does; rows not given stay.
   */
  put<N extends keyof K & string>(kind: N, entities: Iterable<K[N]>, form: Form = 'short'): K[N][] {
    return this.entities.table(kind).upsertMany(entities, form);
  }

  /** Loads the current state of these entities in the form each row holds (for live sync). */
  async refetch(kind: string, ids: number[]): Promise<void> {
    const table = this.entities.find(kind);
    if (!table) return;
    const byForm = new Map<Form, number[]>();
    for (const id of ids) {
      const form = table.form(id);
      if (form) byForm.set(form, [...(byForm.get(form) ?? []), id]);
    }
    await Promise.all([...byForm].map(([form, group]) => this.#load(kind, group, form)));
  }

  /**
   * Edits an entity optimistically: the row shows `changes` at once, and `send` stores them with
   * the version they were made on and returns the entity it saved.
   * - On failure, fields that still hold our values (and no newer server data wrote) roll back.
   * - On `conflict` the entity is fetched again. When the other change left our fields as we
   *   saw them, the edit is sent once more on the new version; otherwise the row keeps the
   *   server's state and a `ConflictError` is thrown.
   */
  async update<N extends keyof K & string>(
    kind: N,
    id: number,
    changes: Changes<K[N]>,
    send: (request: EditRequest<K[N]>) => Promise<K[N]>,
    options: { form?: Form } = {},
  ): Promise<K[N]> {
    const table = this.entities.table(kind);
    const release = table.retain(id);
    try {
      return await this.#attempt(kind, table, id, snapshot(changes), send, options.form, true);
    } finally {
      release();
    }
  }

  async #attempt<N extends keyof K & string>(
    kind: N,
    table: EntityTable<K[N]>,
    id: number,
    changes: Changes<K[N]>,
    send: (request: EditRequest<K[N]>) => Promise<K[N]>,
    form: Form = 'full',
    mayRetry: boolean,
  ): Promise<K[N]> {
    const row = table.get(id) as Record<string, unknown> | undefined;
    if (!row) throw notFound(kind, id);
    const version = row.version as number;
    const keys = Object.keys(changes).filter((key) => key !== 'id' && key !== 'version');
    const base = snapshot(Object.fromEntries(keys.map((key) => [key, row[key]])));
    const ours = changes as Record<string, unknown>;
    const edit = table.beginLocal(id, ours);
    if (!edit) throw notFound(kind, id);

    const rollBack = (): void => {
      const current = table.get(id) as Record<string, unknown> | undefined;
      if (current) {
        for (const key of keys) {
          if (!edit.superseded.has(key) && sameJson(current[key], ours[key])) {
            current[key] = base[key];
          }
        }
      }
      edit.end();
    };

    let saved: K[N];
    try {
      saved = await send({ changes: snapshot(changes), version });
    } catch (error) {
      if (!(error instanceof ApiError && error.code === 'conflict')) {
        rollBack();
        throw error;
      }
      let server: K[N] | undefined;
      try {
        server = await this.#fetchOne(kind, id, table.form(id) ?? form);
      } catch (fetchError) {
        rollBack();
        throw fetchError;
      }
      // From here on everything runs in one tick, so the row never shows a mix.
      rollBack();
      if (!server) {
        table.remove(id);
        throw notFound(kind, id);
      }
      table.upsert(server, table.form(id) ?? form);
      const theirs = server as unknown as Record<string, unknown>;
      if (mayRetry && keys.every((key) => sameJson(theirs[key], base[key]))) {
        return this.#attempt(kind, table, id, changes, send, form, false);
      }
      throw new ConflictError<K[N]>(kind, id, server, changes);
    }
    edit.end();
    return table.upsert(saved, form);
  }

  /** Loads one entity once at a time, keeping a handle promise for callers that wait. */
  #loadEntity(kind: string, id: number, form: Form): Promise<EntityHandle<Entity>> {
    const key = `${kind}:${String(id)}`;
    const running = this.#loads.get(key);
    if (running && covers(running.form, form)) return running.ready;
    const table = this.entities.table(kind as keyof K & string) as unknown as EntityTable<Entity>;
    const ready = this.#load(kind, [id], form)
      .then(() => {
        if (!table.has(id)) throw notFound(kind, id);
        return new EntityHandle(table, id, form);
      })
      .finally(() => {
        if (this.#loads.get(key)?.ready === ready) this.#loads.delete(key);
      });
    this.#loads.set(key, { form, ready });
    return ready;
  }

  /** Loads entities into their table; IDs the server did not return are dropped. */
  async #load(kind: string, ids: number[], form: Form): Promise<void> {
    const loaded = await this.#loader(kind).load(ids, form);
    const table = this.entities.table(kind as keyof K & string) as unknown as EntityTable<Entity>;
    table.upsertMany(loaded, form);
    const found = new Set(loaded.map((entity) => entity.id));
    for (const id of ids) if (!found.has(id)) table.remove(id);
  }

  /** The server's current entity, without storing it. */
  async #fetchOne<T extends Entity>(kind: string, id: number, form: Form): Promise<T | undefined> {
    const [entity] = await this.#loader(kind).load([id], form);
    return entity as T | undefined;
  }

  #loader(kind: string): KindLoader<Entity> {
    const loader = this.#loaders.get(kind);
    if (!loader) throw new Error(`No loader for entity kind "${kind}": call defineKind first`);
    return loader;
  }
}
