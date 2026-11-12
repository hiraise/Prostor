// What the issue list's sidebar shows (QRY-082): the person's drafts, the readable projects
// (favorites first), own and shared tags, saved searches (predefined first, then own, then shared),
// and the issue count of each, all counts in one `POST /issues/counts`. The sidebar lives while the
// list is open, so its data is read in plain requests and read again after a change.
import { ApiError, api } from '#lib/api/client.ts';
import type {
  BuiltinGroupKey,
  CountItem,
  CountQuery,
  Issue,
  Project,
  SavedSearch,
  SharedWith,
  SharedWithInput,
  Tag,
} from '#lib/api/schema.gen.ts';
import type { PersonLike } from '#lib/features/access/index.ts';
import { findProjects } from '#lib/features/projects/index.ts';
import { currentUser } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

/** The server's most queries per count request. */
const COUNTS_MAX = 50;
const LIMIT = 100;

export interface SidebarData {
  drafts: Issue[];
  projects: Project[];
  tags: Tag[];
  searches: SavedSearch[];
}

/** A key per counted entry: `project:1`, `tag:2`, `search:3`. */
export type CountKey = `${'project' | 'tag' | 'search'}:${number}`;

export function countKey(kind: 'project' | 'tag' | 'search', id: number): CountKey {
  return `${kind}:${String(id)}` as CountKey;
}

/** What is shared: a tag (view, use, edit) or a saved search (view, edit). */
export type Shareable = { kind: 'tag'; tag: Tag } | { kind: 'search'; search: SavedSearch };

/** One audience as the sharing dialog edits it: whom an item is shared with for one access. */
export interface Audience {
  users: PersonLike[];
  groups: { id: number; name: string; builtin: BuiltinGroupKey | null; team_of?: number | null }[];
}

function mine(owner: { id: number }): boolean {
  return owner.id === currentUser()?.id;
}

/** Keeps the order of `items` within each rank. */
function ranked<T>(items: readonly T[], rank: (item: T) => number): T[] {
  return items
    .map((item, index) => ({ item, index, rank: rank(item) }))
    .sort((a, b) => a.rank - b.rank || a.index - b.index)
    .map((entry) => entry.item);
}

export async function loadSidebar(signal?: AbortSignal): Promise<SidebarData> {
  const [drafts, projects, tags, searches] = await Promise.all([
    api.get('/issues/drafts', { signal }),
    findProjects(''),
    api.get('/tags', { query: { scope: 'all', limit: LIMIT }, signal }),
    api.get('/saved-searches', { query: { scope: 'all', limit: LIMIT }, signal }),
  ]);
  return {
    drafts: drafts.items,
    projects: ranked(projects.items, (p) => (p.favorite ? 0 : 1)),
    tags: ranked(tags.items, (t) => (mine(t.owner) ? 0 : 1)),
    searches: ranked(searches.items, (s) => (s.builtin !== null ? 0 : mine(s.owner) ? 1 : 2)),
  };
}

/** The counts of every project, tag and search; one request unless there are more than 50. */
export async function loadCounts(
  data: SidebarData,
  signal?: AbortSignal,
): Promise<Map<CountKey, CountItem>> {
  const entries: { key: CountKey; query: CountQuery }[] = [
    ...data.projects.map((p) => ({ key: countKey('project', p.id), query: { project: p.id } })),
    ...data.tags.map((t) => ({ key: countKey('tag', t.id), query: { tag: t.id } })),
    ...data.searches.map((s) => ({
      key: countKey('search', s.id),
      query: { saved_search: s.id },
    })),
  ];
  const chunks: (typeof entries)[] = [];
  for (let i = 0; i < entries.length; i += COUNTS_MAX) {
    chunks.push(entries.slice(i, i + COUNTS_MAX));
  }
  const answers = await Promise.all(
    chunks.map((chunk) =>
      api.post('/issues/counts', { body: { queries: chunk.map((e) => e.query) }, signal }),
    ),
  );
  const counts = new Map<CountKey, CountItem>();
  chunks.forEach((chunk, i) => {
    chunk.forEach((entry, j) => {
      const item = answers[i]?.items[j];
      if (item) counts.set(entry.key, item);
    });
  });
  return counts;
}

/** The number a count shows, or `null` when its query cannot run. */
export function shownCount(item: CountItem | undefined, hideResolved: boolean): number | null {
  if (!item || item.problems.length > 0) return null;
  return hideResolved ? item.unresolved : item.all;
}

export function sharedInput(shared: SharedWith): SharedWithInput {
  return { users: shared.users.map((u) => u.id), groups: shared.groups.map((g) => g.id) };
}

export function isMine(item: Tag | SavedSearch): boolean {
  return mine(item.owner);
}

// ---- Changes (ISS-036, QRY-064, QRY-065, ISS-037) ----

export function updateTag(
  tag: Tag,
  changes: { name?: string; color?: number | null; sharing?: Record<string, SharedWithInput> },
): Promise<Tag> {
  return api.patch('/tags/{id}', {
    path: { id: tag.id },
    body: { ...changes, version: tag.version },
  });
}

export async function tagUsage(id: number): Promise<number> {
  return (await api.get('/tags/{id}/usage', { path: { id } })).issues;
}

export async function deleteTag(id: number): Promise<void> {
  await api.delete('/tags/{id}', { path: { id } });
}

export function updateSearch(
  search: SavedSearch,
  changes: { name?: string; query?: string; sharing?: Record<string, SharedWithInput> },
): Promise<SavedSearch> {
  return api.patch('/saved-searches/{id}', {
    path: { id: search.id },
    body: { ...changes, version: search.version },
  });
}

export async function deleteSearch(id: number): Promise<void> {
  await api.delete('/saved-searches/{id}', { path: { id } });
}

/** Deletes one draft (ISS-011); one already gone counts as deleted. */
export async function deleteDraft(id: number): Promise<void> {
  try {
    await api.delete('/issues/{id}', { path: { id } });
  } catch (error) {
    if (!(error instanceof ApiError && error.code === 'not_found')) throw error;
  }
  store.entities.table('issue').remove(id);
}

export async function deleteAllDrafts(): Promise<void> {
  await api.delete('/issues/drafts');
}
