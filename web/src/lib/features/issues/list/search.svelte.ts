// Loading the issue list (QRY-073, QRY-081): the context of the address, the rows as a cached list
// of the store over `GET /issues` (50 per page, cursor pages), what the server said about the query
// (its notes, the order used) and the count over `GET /issues/count`.
import { SvelteMap } from 'svelte/reactivity';

import { ApiError, api } from '#lib/api/client.ts';
import type {
  Issue,
  IssueCount,
  NewSavedSearch,
  QueryProblem,
  SavedSearch,
  SortUsed,
} from '#lib/api/schema.gen.ts';
import { openProject } from '#lib/features/projects/index.ts';
import { currentUser } from '#lib/session/index.ts';
import { keyText, store, type ListHandle } from '#lib/store/index.ts';

import '../issues.ts';
import type { ListContext, ResolvedContext } from './list-state.svelte.ts';

/** Rows per page: the page the budget is measured on (architecture.md §7). */
export const PAGE = 50;

/** What one search asks the server. */
export interface SearchSpec {
  q: string;
  context: ResolvedContext;
  hideResolved: boolean;
}

/** What the first page of a search said besides its rows. */
export interface SearchMeta {
  notes: QueryProblem[];
  /** The query written canonically. */
  query: string;
  sort: SortUsed[];
  liveTopic: string;
}

const metas = new SvelteMap<string, SearchMeta>();

/** The query parameters of a search's own requests — the export of its every match runs on the
 * same ones, so the file and the list cannot disagree. */
export function params(spec: SearchSpec): {
  q: string | undefined;
  project: number | undefined;
  tag: number | undefined;
  saved_search: number | undefined;
} {
  const { context } = spec;
  return {
    q: spec.q.trim() || undefined,
    project: context.kind === 'project' ? context.id : undefined,
    tag: context.kind === 'tag' ? context.id : undefined,
    saved_search: context.kind === 'search' ? context.id : undefined,
  };
}

function searchKey(spec: SearchSpec): unknown[] {
  return ['issue-search', { ...params(spec), hide: spec.hideResolved }];
}

/** The rows of a search, cached by the store: a repeated search shows at once and reloads. */
export function ensureSearch(spec: SearchSpec): Promise<ListHandle<Issue>> {
  const key = searchKey(spec);
  const text = keyText(key);
  return store.ensureList('issue', {
    key,
    fetch: async (cursor, signal) => {
      const page = await api.get('/issues', {
        query: {
          ...params(spec),
          hide_resolved: spec.hideResolved || undefined,
          limit: PAGE,
          cursor: cursor ?? undefined,
        },
        signal,
      });
      if (cursor === null) {
        metas.set(text, {
          notes: page.notes,
          query: page.query,
          sort: page.sort,
          liveTopic: page.live_topic,
        });
      }
      return page;
    },
  });
}

/** What the server said about a search's query; `undefined` before its first page. Reactive. */
export function searchMeta(spec: SearchSpec): SearchMeta | undefined {
  return metas.get(keyText(searchKey(spec)));
}

/** How many issues a search finds: all and unresolved (QRY-092). */
export function countSearch(spec: SearchSpec, signal?: AbortSignal): Promise<IssueCount> {
  return api.get('/issues/count', { query: params(spec), signal });
}

/** The problems of a refused query (`invalid_query`), with their places in the typed text. */
export function queryProblems(error: unknown): QueryProblem[] | null {
  if (error instanceof ApiError && error.code === 'invalid_query')
    return error.body?.problems ?? [];
  return null;
}

/** What the address's context is; `null` when it does not exist or is hidden. */
export async function resolveContext(context: ListContext): Promise<ResolvedContext | null> {
  switch (context.kind) {
    case 'all':
      return { kind: 'all' };
    case 'project': {
      const opened = await openProject(context.key);
      if (opened.kind === 'missing') return null;
      if (opened.kind === 'moved') return resolveContext({ kind: 'project', key: opened.key });
      const project = opened.project.current;
      if (!project) return null;
      return { kind: 'project', id: project.id, key: project.key, name: project.name };
    }
    case 'tag': {
      const tag = await api.get('/tags/{id}', { path: { id: context.id }, accept: [404] });
      if (!('id' in tag)) return null;
      return { kind: 'tag', id: tag.id, name: tag.name };
    }
    case 'search': {
      const found = await api.get('/saved-searches/{id}', {
        path: { id: context.id },
        accept: [404],
      });
      if (!('id' in found)) return null;
      return searchContext(found);
    }
  }
}

async function searchContext(found: SavedSearch): Promise<ResolvedContext> {
  let projectKey: string | null = null;
  if (found.project !== null) {
    const project = await store
      .ensureEntity('project', found.project, { form: 'full' })
      .then((handle) => handle.current)
      .catch(() => undefined);
    projectKey = project?.key ?? null;
  }
  return {
    kind: 'search',
    id: found.id,
    name: found.name,
    query: found.query,
    projectKey,
    editable: found.abilities.edit,
    mine: found.owner.id === currentUser()?.id,
    version: found.version,
  };
}

/** Saves a query as the person's own search (QRY-054 «Сохранить поиск»). */
export async function saveSearch(body: NewSavedSearch): Promise<ResolvedContext> {
  return searchContext(await api.post('/saved-searches', { body }));
}

/** Gives a saved search a new query (QRY-054 «Обновить поиск»). */
export async function updateSearchQuery(
  id: number,
  query: string,
  version: number,
): Promise<ResolvedContext> {
  return searchContext(
    await api.patch('/saved-searches/{id}', { path: { id }, body: { query, version } }),
  );
}
