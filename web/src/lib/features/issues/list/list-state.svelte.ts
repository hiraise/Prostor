// The issue list's state lives in its address (QRY-069): the query text and one context — a project
// by key, a tag or a saved search by ID (QRY-053). Hide-resolved is the person's preference, not
// part of the address. A copied link and a new saved search keep the query as it reads now: a saved
// search's own query is written out in place of its ID (step 1.7 plan §3.9).
import { resolve } from '$app/paths';
import { SvelteSet } from 'svelte/reactivity';

import type { QueryEngine } from '#lib/query/index.ts';

/** Where the list searches. */
export type ListContext =
  | { kind: 'all' }
  | { kind: 'project'; key: string }
  | { kind: 'tag'; id: number }
  | { kind: 'search'; id: number };

export interface ListParams {
  q: string;
  context: ListContext;
}

/** A context with what the page needs of it, loaded by the route. */
export type ResolvedContext =
  | { kind: 'all' }
  | { kind: 'project'; id: number; key: string; name: string }
  | { kind: 'tag'; id: number; name: string }
  | {
      kind: 'search';
      id: number;
      name: string;
      query: string;
      /** The project the search runs in, by key, when it has one. */
      projectKey: string | null;
      editable: boolean;
      mine: boolean;
      version: number;
    };

function positiveId(text: string | null): number | null {
  if (text === null || !/^\d{1,15}$/.test(text)) return null;
  const id = Number(text);
  return id > 0 ? id : null;
}

/** The state of an address; with several contexts the first of project, tag, search counts. */
export function parseListUrl(url: URL): ListParams {
  const q = url.searchParams.get('q') ?? '';
  const key = url.searchParams.get('project')?.trim();
  if (key) return { q, context: { kind: 'project', key } };
  const tag = positiveId(url.searchParams.get('tag'));
  if (tag !== null) return { q, context: { kind: 'tag', id: tag } };
  const search = positiveId(url.searchParams.get('search'));
  if (search !== null) return { q, context: { kind: 'search', id: search } };
  return { q, context: { kind: 'all' } };
}

/** The address of a list state (relative to the app's base). */
export function listHref(params: ListParams): string {
  // A plain builder, not the reactive Svelte kind: the address is a string of now.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const query = new URLSearchParams();
  const { context } = params;
  if (context.kind === 'project') query.set('project', context.key);
  if (context.kind === 'tag') query.set('tag', String(context.id));
  if (context.kind === 'search') query.set('search', String(context.id));
  if (params.q.trim() !== '') query.set('q', params.q);
  const text = query.toString();
  return text === '' ? resolve('/(app)/issues') : `${resolve('/(app)/issues')}?${text}`;
}

/** The address context of a resolved one. */
export function contextOf(resolved: ResolvedContext): ListContext {
  switch (resolved.kind) {
    case 'all':
      return { kind: 'all' };
    case 'project':
      return { kind: 'project', key: resolved.key };
    case 'tag':
      return { kind: 'tag', id: resolved.id };
    case 'search':
      return { kind: 'search', id: resolved.id };
  }
}

/** A name as the language reads it: bare when it is one plain word, quoted otherwise. */
export function queryWord(name: string): string {
  if (/^[\p{L}\p{N}_-]+$/u.test(name)) return name;
  return `"${name.replace(/[\\"]/g, (c) => `\\${c}`)}"`;
}

const ALTERNATIVES = /(^|[\s)])(or|или)([\s(]|$)/iu;

/** A query split into its conditions and its sort clause. */
function split(engine: QueryEngine, text: string): { conditions: string; sort: string } {
  const place = engine.analyze(text).sort;
  if (!place) return { conditions: text.trim(), sort: '' };
  return {
    conditions: `${text.slice(0, place.from)} ${text.slice(place.to)}`.trim(),
    sort: text.slice(place.from, place.to),
  };
}

/**
 * One query that finds what the list shows now, without the saved search's ID: its query and the
 * typed one joined (a part with `or` in parentheses), the typed sort clause first. A
 * project or tag context is written as a condition when `withContext`; a saved search's project
 * always is. Returns the text and the project key that stays a context (`withContext` false).
 */
export function frozenQuery(
  engine: QueryEngine,
  q: string,
  context: ResolvedContext,
  withContext: boolean,
): { q: string; projectKey: string | null } {
  const typed = split(engine, q);
  const parts: string[] = [];
  let projectKey: string | null = null;
  let sort = typed.sort;
  if (context.kind === 'project') {
    if (withContext) parts.push(`project: ${queryWord(context.key)}`);
    else projectKey = context.key;
  }
  if (context.kind === 'tag') parts.push(`tag: ${queryWord(context.name)}`);
  if (context.kind === 'search') {
    if (context.projectKey !== null) parts.push(`project: ${queryWord(context.projectKey)}`);
    const own = split(engine, context.query);
    if (own.conditions !== '') parts.push(own.conditions);
    if (sort === '') sort = own.sort;
  }
  if (typed.conditions !== '') parts.push(typed.conditions);
  // Conditions side by side all hold; a part with `or` keeps its meaning in parentheses.
  const joined =
    parts.length > 1
      ? parts.map((part) => (ALTERNATIVES.test(part) ? `(${part})` : part)).join(' ')
      : (parts[0] ?? '');
  return { q: [joined, sort].filter((part) => part !== '').join(' '), projectKey };
}

/** The address a copied link holds (QRY-069): the query of now and the context, never a search ID. */
export function copiedHref(engine: QueryEngine, q: string, context: ResolvedContext): string {
  if (context.kind !== 'search') return listHref({ q, context: contextOf(context) });
  const frozen = frozenQuery(engine, q, context, false);
  return listHref({ q: frozen.q, context: { kind: 'all' } });
}

/**
 * The list's keyboard focus (step 2.2 §6, QRY-085): the focused row's issue ID, `null` when the
 * query bar (or nothing) holds it. One instance per list; the focus is by ID, so it survives a
 * refresh — the reloaded answer's rows keep it where they are (D-043).
 */
export class ListFocus {
  current = $state<number | null>(null);
}

/**
 * The list's selection (step 2.2 §8.1, QRY-083): the chosen issues by ID, or the state
 * «everything the query matches». The IDs survive page turns and a refresh; live updates do not
 * touch them — the list waits for «Обновить» (D-043). One instance per list.
 */
export class ListSelection {
  /** The rows chosen one by one; empty while `allMatching` — accepting it lets the IDs go. A
   * `SvelteSet`, not `$state`: Svelte's deep proxy does not take collections. */
  private readonly ids = new SvelteSet<number>();
  /** The state «everything the query matches»: every row renders chosen without an own ID. */
  allMatching = $state(false);
  /** The last row toggled; shift-click and `Shift+↑/↓` extend the range from it. */
  anchor = $state<number | null>(null);

  has(id: number): boolean {
    return this.allMatching || this.ids.has(id);
  }

  get empty(): boolean {
    return !this.allMatching && this.ids.size === 0;
  }

  /** How many rows are chosen by ID; the all-matching count is the query's, not the model's. */
  get size(): number {
    return this.ids.size;
  }

  /** The chosen IDs — empty while `allMatching`: the command dialog's explicit target (§8.2). */
  idsOf(): number[] {
    return [...this.ids];
  }

  /**
   * A row's checkbox: a plain click toggles the row and makes it the anchor; a shift-click takes
   * the range from the anchor. Unchecking a row of an all-matching selection drops to the page's
   * explicit IDs — the rest of the page's rows stay chosen.
   */
  click(id: number, page: readonly number[], shift = false): void {
    if (shift && this.anchor !== null) {
      this.extend(this.anchor, id, page);
      return;
    }
    if (this.allMatching) {
      this.ids.clear();
      for (const row of page) if (row !== id) this.ids.add(row);
      this.allMatching = false;
      this.anchor = id;
      return;
    }
    if (!this.ids.delete(id)) this.ids.add(id);
    this.anchor = id;
  }

  /** Adds every row between two IDs, in the page's order. */
  extend(from: number, to: number, page: readonly number[]): void {
    const start = page.indexOf(from);
    const end = page.indexOf(to);
    if (start === -1 || end === -1) return;
    const range = page.slice(Math.min(start, end), Math.max(start, end) + 1);
    for (const id of range) this.ids.add(id);
  }

  /** `Shift+↑/↓` (§8.1): the anchor — the row the move starts from when no toggle made one —
   * reaches the row the focus lands on. Everything matched stays chosen. */
  keyExtend(from: number, to: number, page: readonly number[]): void {
    if (this.allMatching) return;
    const anchor = this.anchor ?? from;
    this.anchor = anchor;
    this.extend(anchor, to, page);
  }

  /** The header's checkbox: the page's issues all chosen, or the selection let go. */
  selectPage(all: boolean, page: readonly number[]): void {
    if (!all) {
      this.clear();
      return;
    }
    for (const id of page) this.ids.add(id);
    this.anchor = null;
  }

  /** «Выбрать все N задач по запросу»: the query's worth, no own IDs. */
  selectAllMatching(): void {
    this.allMatching = true;
    this.ids.clear();
    this.anchor = null;
  }

  clear(): void {
    this.ids.clear();
    this.allMatching = false;
    this.anchor = null;
  }
}
