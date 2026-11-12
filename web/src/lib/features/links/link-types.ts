// Link types in the data store (architecture.md §15; LNK-018…LNK-021): the `link_type` kind, the
// list of the administration page and its calls. An instance has a handful of types, so the list
// is one request and a refetch of some types loads the list again.
import { api } from '#lib/api/client.ts';
import type { LinkDirection, LinkType, LinkTypeImpact, NewLinkType } from '#lib/api/schema.gen.ts';
import { store, type ListHandle } from '#lib/store/index.ts';

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    link_type: LinkType;
  }
}

store.defineKind('link_type', {
  async load(ids) {
    const { items } = await api.get('/link-types');
    return items.filter((type) => ids.includes(type.id));
  },
});

/** Directions in the order the dialog offers them (LNK-019). */
export const DIRECTIONS: readonly LinkDirection[] = ['undirected', 'directed', 'hierarchy'];

/** Every link type, oldest first; deleted ones too for holders of `system.read` (LNK-018). */
export function ensureLinkTypes(): Promise<ListHandle<LinkType>> {
  return store.ensureList('link_type', {
    key: ['link-types'],
    form: 'full',
    fetch: async (_cursor, signal) => {
      const { items } = await api.get('/link-types', { signal });
      return { items, next_cursor: null };
    },
  });
}

/** The stored row of a type, if loaded. Reactive. */
export function linkTypeRow(id: number): LinkType | undefined {
  return store.entities.table('link_type').get(id);
}

/** Creates a link type (LNK-019). */
export async function createLinkType(input: NewLinkType): Promise<LinkType> {
  const type = await api.post('/link-types', { body: input });
  return store.entities.table('link_type').upsert(type, 'full');
}

/** What the dialog changes on a type (LNK-020). */
export type LinkTypeChanges = Partial<
  Pick<LinkType, 'name' | 'source_name' | 'target_name' | 'direction'>
>;

/**
 * Saves changes of a type on the version the row holds; the list shows them at once. Throws
 * `ConflictError` when someone else changed the same properties meanwhile. An undirected type
 * has no target name: the server drops it, so it is not sent.
 */
export function updateLinkType(id: number, changes: LinkTypeChanges): Promise<LinkType> {
  return store.update('link_type', id, changes, ({ changes: sent, version }) =>
    api.patch('/link-types/{id}', {
      path: { id },
      body: {
        name: sent.name,
        source_name: sent.source_name,
        target_name: sent.target_name ?? undefined,
        direction: sent.direction,
        version,
      },
    }),
  );
}

/** How many links deleting a type hides (LNK-021). */
export function linkTypeImpact(id: number, signal?: AbortSignal): Promise<LinkTypeImpact> {
  return api.get('/link-types/{id}/impact', { path: { id }, signal });
}

/**
 * Puts a type in the trash: its links are hidden until it is restored (LNK-021). The list shows
 * it as deleted after its next refresh.
 */
export async function deleteLinkType(id: number): Promise<void> {
  await api.delete('/link-types/{id}', { path: { id } });
}

/** Takes a type out of the trash with its links. */
export async function restoreLinkType(id: number): Promise<LinkType> {
  const type = await api.post('/link-types/{id}/restore', { path: { id } });
  return store.entities.table('link_type').upsert(type, 'full');
}
