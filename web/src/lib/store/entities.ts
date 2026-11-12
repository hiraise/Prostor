// Entity tables (architecture.md §15): one per kind, rows keyed by integer ID with the entity's
// `version`. Rows are deep reactive proxies updated in place, so a component holding a row sees
// its field changes and a component reading one row is not woken by another. Every field of a row
// belongs to the row's version; the row also knows which form (short or full) it holds.
import { SvelteMap } from 'svelte/reactivity';

import { reactive, snapshot } from './reactive.svelte.ts';
import { Retention } from './retention.ts';

/** What every stored entity has: a stable ID and the version of its data. */
export interface Entity {
  id: number;
  version: number;
}

/** Response shapes (architecture.md §4): `short` in lists and references, `full` on its own. */
export type Form = 'short' | 'full';

/** Whether data in form `have` satisfies a need for form `need`. */
export function covers(have: Form | undefined, need: Form): boolean {
  return have === 'full' || (have === 'short' && need === 'short');
}

/** What `applyPatch` did. */
export type PatchResult =
  /** The patch was applied; the row now has the event's version. */
  | 'applied'
  /** The row already has this version or a newer one; nothing changed. */
  | 'current'
  /** The row is older than the version before the patch: the patch alone would be wrong. */
  | 'stale'
  /** The table has no such row. */
  | 'missing';

/** Optimistic values written into a row before the server confirmed them. */
export interface LocalEdit {
  /** Fields a newer server version wrote after the edit started: rollback leaves them alone. */
  readonly superseded: ReadonlySet<string>;
  /** The edit is over (confirmed or rolled back): server data of its version may write again. */
  end(): void;
}

export interface TableOptions {
  /** How long a row nobody retains stays; `Infinity` keeps rows until removed. */
  evictAfterMs?: number;
  /** A row someone needs in full form was replaced by a newer short form. */
  onDowngrade?: (id: number) => void;
  now?: () => number;
}

type Row = Record<string, unknown>;

interface RowInfo {
  form: Form;
  fetchedAt: number;
}

interface EditState {
  readonly fields: ReadonlySet<string>;
  readonly superseded: Set<string>;
}

/** Entities of one kind. */
export class EntityTable<T extends Entity> {
  readonly kind: string;
  readonly #rows = new SvelteMap<number, T>();
  readonly #info = new Map<number, RowInfo>();
  /** Holders that need the full form, per row. */
  readonly #fullNeeds = new Map<number, number>();
  readonly #edits = new Map<number, Set<EditState>>();
  readonly #retention: Retention<number>;
  readonly #onDowngrade: ((id: number) => void) | undefined;
  readonly #now: () => number;

  constructor(kind: string, options: TableOptions = {}) {
    this.kind = kind;
    this.#retention = new Retention(options.evictAfterMs ?? Infinity, (id) => {
      this.remove(id);
    });
    this.#onDowngrade = options.onDowngrade;
    this.#now = options.now ?? Date.now;
  }

  /** The row with this ID; reading it inside a component or `$derived` tracks only this ID. */
  get(id: number): T | undefined {
    return this.#rows.get(id);
  }

  has(id: number): boolean {
    return this.#rows.has(id);
  }

  /** Number of rows; reading it tracks additions and removals. */
  get size(): number {
    return this.#rows.size;
  }

  /** The form the row holds; not reactive. */
  form(id: number): Form | undefined {
    return this.#info.get(id)?.form;
  }

  /** When the row was last written by the server; not reactive. */
  fetchedAt(id: number): number | undefined {
    return this.#info.get(id)?.fetchedAt;
  }

  /**
   * Stores an entity from the server in the given form and returns the stored row.
   * - An older version than the stored one is ignored.
   * - The same version merges: it adds the fields the row lacks and keeps the better form.
   *   Fields with optimistic values in flight keep them.
   * - A newer version replaces the row's fields and form, dropping fields the new data lacks
   *   (a newer short form downgrades a full row).
   */
  upsert(entity: T, form: Form = 'short'): T {
    const incoming = snapshot(entity) as unknown as Row;
    const { id } = entity;
    const stored = this.#rows.get(id);
    const now = this.#now();
    if (!stored) {
      const row = reactive(incoming) as unknown as T;
      this.#rows.set(id, row);
      this.#info.set(id, { form, fetchedAt: now });
      this.#retention.track(id);
      return row;
    }
    if (entity.version < stored.version) return stored;

    const row = stored as unknown as Row;
    const info = this.#info.get(id) ?? { form, fetchedAt: now };
    this.#info.set(id, info);
    info.fetchedAt = now;
    if (entity.version > stored.version) {
      for (const key of Object.keys(row)) {
        if (!(key in incoming)) Reflect.deleteProperty(row, key);
      }
      this.#markSuperseded(id, null);
      assign(row, incoming);
      const downgraded = info.form === 'full' && form === 'short';
      info.form = form;
      if (downgraded && (this.#fullNeeds.get(id) ?? 0) > 0) this.#onDowngrade?.(id);
    } else {
      const local = this.#localFields(id);
      for (const key of local) Reflect.deleteProperty(incoming, key);
      assign(row, incoming);
      if (form === 'full') info.form = 'full';
    }
    return stored;
  }

  upsertMany(entities: Iterable<T>, form: Form = 'short'): T[] {
    return Array.from(entities, (entity) => this.upsert(entity, form));
  }

  /**
   * Applies the changed fields of the change that produced `version`. It succeeds only when the
   * row is exactly one version behind; otherwise the caller fetches the entity.
   */
  applyPatch(id: number, version: number, patch: Readonly<Record<string, unknown>>): PatchResult {
    const stored = this.#rows.get(id);
    if (!stored) return 'missing';
    if (stored.version >= version) return 'current';
    if (stored.version !== version - 1) return 'stale';
    const fields = snapshot(patch) as Row;
    delete fields.id;
    delete fields.version;
    this.#markSuperseded(id, Object.keys(fields));
    const row = stored as unknown as Row;
    assign(row, fields);
    row.version = version;
    const info = this.#info.get(id);
    if (info) info.fetchedAt = this.#now();
    return 'applied';
  }

  /**
   * Writes optimistic values into a row; returns null when the row is not loaded. Until `end()`,
   * data of the same version does not overwrite these fields, and newer data that writes them
   * marks them as superseded.
   */
  beginLocal(id: number, fields: Readonly<Record<string, unknown>>): LocalEdit | null {
    const stored = this.#rows.get(id);
    if (!stored) return null;
    const values = snapshot(fields) as Row;
    delete values.id;
    delete values.version;
    const state: EditState = { fields: new Set(Object.keys(values)), superseded: new Set() };
    let edits = this.#edits.get(id);
    if (!edits) {
      edits = new Set();
      this.#edits.set(id, edits);
    }
    edits.add(state);
    assign(stored as unknown as Row, values);
    return {
      superseded: state.superseded,
      end: () => {
        const active = this.#edits.get(id);
        active?.delete(state);
        if (active?.size === 0) this.#edits.delete(id);
      },
    };
  }

  /**
   * Keeps the row from eviction until the returned function is called. `need: 'full'` also asks
   * for the full form to be fetched again if a newer short form replaces it.
   */
  retain(id: number, need: Form = 'short'): () => void {
    const release = this.#retention.retain(id);
    if (need === 'short') return release;
    this.#fullNeeds.set(id, (this.#fullNeeds.get(id) ?? 0) + 1);
    let held = true;
    return () => {
      if (!held) return;
      held = false;
      const count = (this.#fullNeeds.get(id) ?? 1) - 1;
      if (count > 0) this.#fullNeeds.set(id, count);
      else this.#fullNeeds.delete(id);
      release();
    };
  }

  remove(id: number): boolean {
    this.#info.delete(id);
    this.#retention.forget(id);
    return this.#rows.delete(id);
  }

  clear(): void {
    for (const id of [...this.#rows.keys()]) this.remove(id);
  }

  #localFields(id: number): Set<string> {
    const fields = new Set<string>();
    for (const edit of this.#edits.get(id) ?? []) for (const key of edit.fields) fields.add(key);
    return fields;
  }

  /** Server data of a newer version writes `keys` (null: every field). */
  #markSuperseded(id: number, keys: readonly string[] | null): void {
    for (const edit of this.#edits.get(id) ?? []) {
      for (const key of edit.fields) {
        if (keys === null || keys.includes(key)) edit.superseded.add(key);
      }
    }
  }
}

/** Table options per kind; `onDowngrade` also gets the kind. */
export interface StoreOptions extends Omit<TableOptions, 'onDowngrade'> {
  onDowngrade?: (kind: string, id: number) => void;
}

/** Entity kinds a store holds: kind name → entity type. */
export type KindMap<K> = { [N in keyof K]: Entity };

/** All entity tables of the app. Tables are created on first use. */
export class EntityStore<K extends KindMap<K> = Record<string, Entity>> {
  readonly #tables = new Map<string, EntityTable<Entity>>();
  readonly #options: StoreOptions;

  constructor(options: StoreOptions = {}) {
    this.#options = options;
  }

  /** The table of a kind, created on first use. */
  table<N extends keyof K & string>(kind: N): EntityTable<K[N]> {
    let table = this.#tables.get(kind);
    if (!table) {
      const { onDowngrade, ...options } = this.#options;
      table = new EntityTable<Entity>(kind, {
        ...options,
        onDowngrade:
          onDowngrade &&
          ((id) => {
            onDowngrade(kind, id);
          }),
      });
      this.#tables.set(kind, table);
    }
    return table as unknown as EntityTable<K[N]>;
  }

  /** The table of a kind if anything was stored in it; for events whose kind is only a string. */
  find(kind: string): EntityTable<Entity> | undefined {
    return this.#tables.get(kind);
  }

  clear(): void {
    for (const table of this.#tables.values()) table.clear();
  }
}

/** Writes changed fields only, so unchanged values do not wake their readers. */
function assign(row: Row, fields: Row): void {
  for (const [key, value] of Object.entries(fields)) {
    if (!sameJson(row[key], value)) row[key] = value;
  }
}

/** Deep equality of JSON values (what the API returns). */
export function sameJson(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a)) {
    return Array.isArray(b) && a.length === b.length && a.every((item, i) => sameJson(item, b[i]));
  }
  if (Array.isArray(b)) return false;
  const left = a as Row;
  const right = b as Row;
  const keys = Object.keys(left);
  return (
    keys.length === Object.keys(right).length &&
    keys.every((key) => key in right && sameJson(left[key], right[key]))
  );
}
