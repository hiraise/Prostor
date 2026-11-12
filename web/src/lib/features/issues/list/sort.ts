// Sorting the list by a column header (QRY-075): the query's sort clause, found by the language
// (`Analysis.sort`), is replaced by one key — ascending first, descending on the next click.
import type { SortUsed } from '#lib/api/schema.gen.ts';

import type { Column } from './columns.ts';

export type Direction = 'asc' | 'desc';

/** `q` with its sort clause (at `place`, or none) replaced by `sort: <name> <direction>`. */
export function sortedQuery(
  q: string,
  place: { from: number; to: number } | null,
  name: string,
  direction: Direction,
): string {
  const clause = `sort: ${name} ${direction}`;
  if (place === null) return q.trim() === '' ? clause : `${q.trimEnd()} ${clause}`;
  return `${q.slice(0, place.from)}${clause}${q.slice(place.to)}`;
}

/** The direction a click on a header gives: ascending unless the list is ascending by it now. */
export function nextDirection(current: Direction | null): Direction {
  return current === 'asc' ? 'desc' : 'asc';
}

const KEYS: Partial<Record<string, SortUsed['key']>> = {
  id: 'id',
  summary: 'summary',
  project: 'project',
  reporter: 'reporter',
  created: 'created',
  updated: 'updated',
  resolved: 'resolved',
  votes: 'votes',
};

/** How the list is sorted by `column` now: by the first key of the order the server used. */
export function sortedBy(column: Column, sort: readonly SortUsed[]): Direction | null {
  const first = sort[0];
  if (!first) return null;
  const matches =
    column.kind === 'field'
      ? first.key === 'field' && first.field === column.id
      : KEYS[column.name] === first.key;
  return matches ? first.direction : null;
}
