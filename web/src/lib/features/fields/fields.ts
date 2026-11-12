// Custom fields and value sets in the data store (architecture.md §15; FLD-015…FLD-070): the
// `field` and `value_set` kinds, their lists and filters, and the calls of the fields pages. The
// fields of one project are plain requests (`project-fields.svelte.ts`): a project's settings of a
// field have no ID of their own, and only the page that shows them needs them.
import { api } from '#lib/api/client.ts';
import type {
  AttachField,
  Field,
  FieldBoards,
  FieldDeletionImpact,
  FieldKind,
  IssueImpact,
  NewField,
  NewValue,
  PageField,
  PageValueSetSummary,
  ProjectField,
  ProjectFieldPatch,
  ProjectFields,
  SetKind,
  SetMembersChange,
  UsedIn,
  ValueSet,
  ValueSetSummary,
  SetValue as GeneratedSetValue,
} from '#lib/api/schema.gen.ts';
import { store, type EntityHandle, type ListHandle } from '#lib/store/index.ts';

/** A value of a set (FLD-043…FLD-049); properties of other kinds keep their defaults. */
export type SetValue = GeneratedSetValue;

/** A set as the store keeps it: the list's summary, or the full set with its values. */
export type StoredSet = ValueSetSummary | ValueSet;

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    field: Field;
    value_set: StoredSet;
  }
}

/** Fields per page: an instance has tens of them, so one page holds them all. */
const FIELDS_PAGE = 200;
/** Sets per page of the sets list. */
const SETS_PAGE = 100;
/** Fields per page of the "add an existing field" search. */
const FIND_PAGE = 50;

store.defineKind('field', {
  // There is no batch endpoint: a refetch loads each field; they are few at a time.
  async load(ids) {
    const found = await Promise.all(
      ids.map((id) =>
        api
          .get('/fields/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    return found.filter((field): field is Field => field !== null);
  },
});

store.defineKind('value_set', {
  async load(ids) {
    const found = await Promise.all(
      ids.map((id) =>
        api
          .get('/value-sets/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    return found.filter((set): set is ValueSet => set !== null);
  },
});

// ---- Kinds of fields (FLD-001) ----

/** Every kind, in the order pickers offer them: simple ones first, then those with values. */
export const FIELD_KINDS: readonly FieldKind[] = [
  'string',
  'text',
  'integer',
  'float',
  'date',
  'date_time',
  'duration',
  'enum',
  'state',
  'user',
  'group',
  'version',
  'build',
  'owned',
];

/** Kinds of value sets (FLD-029). */
export const SET_KINDS: readonly SetKind[] = ['enum', 'state', 'user', 'version', 'build', 'owned'];

/** Kinds that may hold several values (FLD-001): enumerated ones except state. */
export function mayBeMulti(kind: FieldKind): boolean {
  return ['enum', 'user', 'group', 'version', 'build', 'owned'].includes(kind);
}

/** The kind of set a field of this kind takes its values from; `null` for simple kinds and groups. */
export function setKindOf(kind: FieldKind): SetKind | null {
  return (SET_KINDS as readonly string[]).includes(kind) ? (kind as SetKind) : null;
}

/** The values of a full set, in order (empty for user sets). */
export function setValues(set: ValueSet): SetValue[] {
  return set.values;
}

/** The set row holds the full set with its values and members. */
export function isFullSet(set: StoredSet | undefined): set is ValueSet {
  return set !== undefined && 'values' in set;
}

/** Projects a set is used in, the hidden ones included. */
export function useCount(set: ValueSet): number {
  return set.used_in.items.length + set.used_in.hidden;
}

/** Aliases typed as one line: separated by commas, blanks dropped, each once. */
export function parseAliases(text: string): string[] {
  const seen = new Set<string>();
  const aliases: string[] = [];
  for (const raw of text.split(',')) {
    const alias = raw.trim();
    if (alias && !seen.has(alias.toLowerCase())) {
      seen.add(alias.toLowerCase());
      aliases.push(alias);
    }
  }
  return aliases;
}

// ---- Fields of the instance (FLD-015…FLD-025) ----

/** What the fields list shows: part of a name or an alias, and only unused fields. */
export interface FieldFilters {
  q: string;
  unused: boolean;
}

/** The filters of the fields page from its address (UI-006). */
export function fieldFilters(params: URLSearchParams): FieldFilters {
  return { q: params.get('q')?.trim() ?? '', unused: params.get('unused') === 'true' };
}

/** Fields matching the filters, in the instance order (FLD-022, FLD-025). */
export function ensureFields(filters: FieldFilters): Promise<ListHandle<Field>> {
  const q = filters.q.trim();
  return store.ensureList('field', {
    key: ['fields', { q, unused: filters.unused }],
    form: 'full',
    fetch: (cursor, signal) =>
      api.get('/fields', {
        query: {
          q: q || undefined,
          unused: filters.unused || undefined,
          limit: FIELDS_PAGE,
          cursor: cursor ?? undefined,
        },
        signal,
      }),
  });
}

/** One field; rejects with `not_found` when it does not exist or is hidden. */
export function ensureField(id: number): Promise<EntityHandle<Field>> {
  return store.ensureEntity('field', id, { form: 'full' });
}

/** The stored row of a field, if loaded. Reactive. */
export function fieldRow(id: number): Field | undefined {
  return store.entities.table('field').get(id);
}

/** One page of fields by name or alias, for choosing one to attach (FLD-053); not stored. */
export function findFields(text: string, cursor?: string | null): Promise<PageField> {
  return api.get('/fields', {
    query: { q: text.trim() || undefined, limit: FIND_PAGE, cursor: cursor ?? undefined },
  });
}

/** The projects using a field that the viewer may see, and how many others (FLD-025). */
export function fieldProjects(id: number, signal?: AbortSignal): Promise<UsedIn> {
  return api.get('/fields/{id}/projects', { path: { id }, signal });
}

/** The boards using a field and how, and how many others the viewer does not view (BRD-022). */
export function fieldBoards(id: number, signal?: AbortSignal): Promise<FieldBoards> {
  return api.get('/fields/{id}/boards', { path: { id }, signal });
}

/** What deleting a field takes away (FLD-023). */
export function fieldDeletionImpact(id: number): Promise<FieldDeletionImpact> {
  return api.get('/fields/{id}/deletion-impact', { path: { id } });
}

/** Creates a field of the instance (FLD-015). */
export async function createField(input: NewField): Promise<Field> {
  const field = await api.post('/fields', { body: input });
  return store.entities.table('field').upsert(field, 'full');
}

/** What the field page changes (FLD-016…FLD-020). */
export type FieldChanges = Partial<
  Pick<
    Field,
    | 'name'
    | 'aliases'
    | 'auto_attach'
    | 'can_be_empty'
    | 'empty_label'
    | 'private'
    | 'default_values'
    | 'default_users'
  >
>;

/**
 * Saves changes of a field on the version the row holds; the row shows them at once. Throws
 * `ConflictError` when someone else changed the same properties meanwhile.
 */
export function updateField(id: number, changes: FieldChanges): Promise<Field> {
  return store.update('field', id, changes, ({ changes: sent, version }) => {
    const defaults = sent.default_users ?? sent.default_values;
    return api.patch('/fields/{id}', {
      path: { id },
      body: {
        name: sent.name,
        aliases: sent.aliases,
        auto_attach: sent.auto_attach,
        can_be_empty: sent.can_be_empty,
        empty_label: sent.empty_label,
        private: sent.private,
        default_values: defaults?.map((item) => item.id),
        version,
      },
    });
  });
}

/** Moves a field right after another one in the instance order, or first (FLD-022). */
export async function moveField(id: number, after: number | null): Promise<Field> {
  const field = await api.post('/fields/{id}/move', { path: { id }, body: { after } });
  return store.entities.table('field').upsert(field, 'full');
}

/** Deletes a field from every project (FLD-023). The row stays until `forgetField`. */
export async function deleteField(id: number): Promise<void> {
  await api.delete('/fields/{id}', { path: { id } });
}

/** Drops a deleted field from the store, and so from every list that shows it. */
export function forgetField(id: number): void {
  store.entities.table('field').remove(id);
}

// ---- Value sets (FLD-029…FLD-049) ----

/** What the sets list shows: part of a name and a kind ('' for any). */
export interface SetFilters {
  q: string;
  kind: SetKind | '';
}

/** The filters of the sets page from its address (UI-006); unknown kinds are ignored. */
export function setFilters(params: URLSearchParams): SetFilters {
  const kind = params.get('kind') ?? '';
  return {
    q: params.get('q')?.trim() ?? '',
    kind: (SET_KINDS as readonly string[]).includes(kind) ? (kind as SetKind) : '',
  };
}

/** Sets matching the filters, by name (FLD-025). */
export function ensureValueSets(filters: SetFilters): Promise<ListHandle<StoredSet>> {
  const q = filters.q.trim();
  return store.ensureList('value_set', {
    key: ['value-sets', { q, kind: filters.kind }],
    fetch: (cursor, signal) =>
      api.get('/value-sets', {
        query: {
          q: q || undefined,
          kind: filters.kind || undefined,
          limit: SETS_PAGE,
          cursor: cursor ?? undefined,
        },
        signal,
      }),
  });
}

/** One set with its values or members; rejects with `not_found` when it does not exist. */
export function ensureValueSet(id: number): Promise<EntityHandle<StoredSet>> {
  return store.ensureEntity('value_set', id, { form: 'full' });
}

/** Sets of one kind for choosing one (FLD-015); not stored. */
export function findValueSets(kind: SetKind, cursor?: string | null): Promise<PageValueSetSummary> {
  return api.get('/value-sets', {
    query: { kind, limit: SETS_PAGE, cursor: cursor ?? undefined },
  });
}

/** Puts a set the server returned into the store. */
export function keepSet(set: ValueSet): ValueSet {
  return store.entities.table('value_set').upsert(set, 'full') as ValueSet;
}

/** Creates an empty set (FLD-029). */
export async function createValueSet(name: string, kind: SetKind): Promise<ValueSet> {
  return keepSet(await api.post('/value-sets', { body: { name, kind } }));
}

/** Renames a set; `ConflictError` when someone else renamed it meanwhile. */
export async function renameValueSet(id: number, name: string): Promise<ValueSet> {
  const saved = await store.update('value_set', id, { name }, ({ changes: sent, version }) =>
    api.patch('/value-sets/{id}', {
      path: { id },
      body: { name: sent.name ?? name, version },
    }),
  );
  return saved as ValueSet;
}

/** Deletes a set nobody uses. The row stays until `forgetValueSet`. */
export async function deleteValueSet(id: number): Promise<void> {
  await api.delete('/value-sets/{id}', { path: { id } });
}

/** Drops a deleted set from the store, and so from every list that shows it. */
export function forgetValueSet(id: number): void {
  store.entities.table('value_set').remove(id);
}

/** Adds a value right after another one, or at the end (FLD-032). */
export async function addValue(setId: number, input: NewValue): Promise<ValueSet> {
  return keepSet(await api.post('/value-sets/{id}/values', { path: { id: setId }, body: input }));
}

/** What the set editor changes on a value (FLD-043…FLD-049). */
export type ValueChanges = Partial<
  Pick<
    SetValue,
    | 'name'
    | 'description'
    | 'color'
    | 'archived'
    | 'resolved'
    | 'released'
    | 'release_date'
    | 'start_date'
    | 'assembled_on'
    | 'owner'
  >
>;

/**
 * Shows new `values` of a set at once while `send` stores them; on failure the set shows what it
 * had, unless newer server data arrived meanwhile.
 */
async function withValues(
  setId: number,
  values: SetValue[],
  send: () => Promise<ValueSet>,
): Promise<ValueSet> {
  const table = store.entities.table('value_set');
  const row = table.get(setId);
  const before = isFullSet(row) ? setValues(row) : null;
  const edit = before ? table.beginLocal(setId, { values }) : null;
  try {
    const saved = await send();
    edit?.end();
    return keepSet(saved);
  } catch (error) {
    const now = table.get(setId);
    if (edit && before && isFullSet(now) && !edit.superseded.has('values')) now.values = before;
    edit?.end();
    throw error;
  }
}

/** Changes a value on the version it was shown with; the set shows the change at once. */
export function editValue(
  setId: number,
  value: SetValue,
  changes: ValueChanges,
): Promise<ValueSet> {
  const row = store.entities.table('value_set').get(setId);
  const shown = isFullSet(row)
    ? setValues(row).map((item) => (item.id === value.id ? { ...item, ...changes } : item))
    : [];
  const { owner, ...rest } = changes;
  return withValues(setId, shown, () =>
    api.patch('/value-sets/{id}/values/{value_id}', {
      path: { id: setId, value_id: value.id },
      body: {
        ...rest,
        ...(owner === undefined ? {} : { owner_id: owner?.id ?? null }),
        version: value.version,
      },
    }),
  );
}

/** Moves a value right after another one, or first (FLD-037); the set shows the order at once. */
export function moveValue(setId: number, valueId: number, after: number | null): Promise<ValueSet> {
  const row = store.entities.table('value_set').get(setId);
  const shown = isFullSet(row) ? reorder(setValues(row), (value) => value.id, valueId, after) : [];
  return withValues(setId, shown, () =>
    api.post('/value-sets/{id}/values/{value_id}/move', {
      path: { id: setId, value_id: valueId },
      body: { after },
    }),
  );
}

/** How many issues hold a value (FLD-032). */
export function valueUsage(
  setId: number,
  valueId: number,
  signal?: AbortSignal,
): Promise<IssueImpact> {
  return api.get('/value-sets/{id}/values/{value_id}/usage', {
    path: { id: setId, value_id: valueId },
    signal,
  });
}

/**
 * Deletes a value (FLD-032); it stops being a default anywhere. Issues holding it get `replacement`,
 * another value of the set, or lose it; the server updates them in the background.
 */
export async function deleteValue(
  setId: number,
  valueId: number,
  replacement: number | null = null,
): Promise<ValueSet> {
  return keepSet(
    await api.delete('/value-sets/{id}/values/{value_id}', {
      path: { id: setId, value_id: valueId },
      query: { replacement: replacement ?? undefined },
    }),
  );
}

/** Adds accounts and groups to a user set (FLD-040). */
export async function addSetMembers(setId: number, change: SetMembersChange): Promise<ValueSet> {
  return keepSet(await api.post('/value-sets/{id}/members', { path: { id: setId }, body: change }));
}

/** Removes accounts and groups from a user set (FLD-040). */
export async function removeSetMembers(setId: number, change: SetMembersChange): Promise<ValueSet> {
  return keepSet(
    await api.post('/value-sets/{id}/members/remove', { path: { id: setId }, body: change }),
  );
}

// ---- Fields of a project (FLD-051…FLD-060) ----

/** The fields of a project in its order (FLD-051). */
export function fetchProjectFields(
  projectId: number,
  signal?: AbortSignal,
): Promise<ProjectFields> {
  return api.get('/projects/{id}/fields', { path: { id: projectId }, signal });
}

/** Attaches an existing field or creates one for the project (FLD-052, FLD-053). */
export function attachField(projectId: number, input: AttachField): Promise<ProjectField> {
  return api.post('/projects/{id}/fields', { path: { id: projectId }, body: input });
}

/** Changes a field's settings in the project (FLD-055, FLD-056, FLD-060). */
export function updateProjectField(
  projectId: number,
  fieldId: number,
  patch: ProjectFieldPatch,
): Promise<ProjectField> {
  return api.patch('/projects/{id}/fields/{field_id}', {
    path: { id: projectId, field_id: fieldId },
    body: patch,
  });
}

/** Moves a field in the project's order (FLD-058); answers every field in the new order. */
export function moveProjectField(
  projectId: number,
  fieldId: number,
  after: number | null,
): Promise<ProjectFields> {
  return api.post('/projects/{id}/fields/{field_id}/move', {
    path: { id: projectId, field_id: fieldId },
    body: { after },
  });
}

/** How many of the project's issues lose a value when the field is detached (FLD-054). */
export function detachImpact(
  projectId: number,
  fieldId: number,
  signal?: AbortSignal,
): Promise<IssueImpact> {
  return api.get('/projects/{id}/fields/{field_id}/detach-impact', {
    path: { id: projectId, field_id: fieldId },
    signal,
  });
}

/** Detaches a field from the project (FLD-054). */
export async function detachField(projectId: number, fieldId: number): Promise<void> {
  await api.delete('/projects/{id}/fields/{field_id}', {
    path: { id: projectId, field_id: fieldId },
  });
}

/** Gives the project its own copy of a shared set (FLD-030). */
export function forkFieldSet(projectId: number, fieldId: number): Promise<ProjectField> {
  return api.post('/projects/{id}/fields/{field_id}/fork-set', {
    path: { id: projectId, field_id: fieldId },
  });
}

// ---- Order (FLD-022, FLD-037, FLD-058) ----

/** `items` with the item `id` moved right after `after` (`null`: first). */
export function reorder<T>(
  items: readonly T[],
  key: (item: T) => number,
  id: number,
  after: number | null,
): T[] {
  const moving = items.find((item) => key(item) === id);
  if (moving === undefined) return [...items];
  const rest = items.filter((item) => key(item) !== id);
  const index = after === null ? 0 : rest.findIndex((item) => key(item) === after) + 1;
  if (after !== null && index === 0) return [...items];
  return [...rest.slice(0, index), moving, ...rest.slice(index)];
}

/**
 * Where "Move up" puts `id`: right after the item before the previous shown one (`null`: first).
 * `shown` is the order on screen (a filter may hide items); `undefined` when it is first already.
 */
export function afterMovingUp(
  all: readonly number[],
  id: number,
  shown: readonly number[] = all,
): number | null | undefined {
  const index = shown.indexOf(id);
  const previous = shown[index - 1];
  if (index <= 0 || previous === undefined) return undefined;
  const position = all.indexOf(previous);
  return position > 0 ? (all[position - 1] ?? null) : null;
}

/** Where "Move down" puts `id`: right after the next shown item; `undefined` when last. */
export function afterMovingDown(
  all: readonly number[],
  id: number,
  shown: readonly number[] = all,
): number | undefined {
  const index = shown.indexOf(id);
  if (index < 0) return undefined;
  return shown[index + 1];
}

/**
 * Where dropping `id` before or after `target` puts it, as "right after" (`null`: first);
 * `undefined` when the order would not change.
 */
export function afterDropping(
  all: readonly number[],
  id: number,
  target: number,
  side: 'before' | 'after',
): number | null | undefined {
  if (id === target || !all.includes(id) || !all.includes(target)) return undefined;
  let after: number | null;
  if (side === 'after') {
    after = target;
  } else {
    const position = all.indexOf(target);
    after = position > 0 ? (all[position - 1] ?? null) : null;
  }
  const current = all.indexOf(id);
  const before = current > 0 ? (all[current - 1] ?? null) : null;
  if (after === id || after === before) return undefined;
  return after;
}
