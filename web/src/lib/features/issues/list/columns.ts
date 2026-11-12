// The columns of the issue list (QRY-075, QRY-078): the person's choice in preference
// `list_columns` (`id`, `summary`, built-in attributes, `field:<id>`); none chosen means ID,
// summary and the first three fields of the context project — of all projects, the three most used.
import { api } from '#lib/api/client.ts';
import type { FieldRef } from '#lib/api/schema.gen.ts';

import { queryWord } from './list-state.ts';
import * as m from './messages.ts';

export const BUILTIN_COLUMNS = [
  'id',
  'summary',
  'project',
  'reporter',
  'created',
  'updated',
  'resolved',
  'votes',
  'tags',
] as const;

export type BuiltinColumn = (typeof BUILTIN_COLUMNS)[number];

export type Column = { kind: 'builtin'; name: BuiltinColumn } | { kind: 'field'; id: number };

/** A column with what the list shows of it. */
export interface ShownColumn {
  column: Column;
  /** Its preference text. */
  text: string;
  /** Stable among the columns: field IDs, negative numbers for the built-in ones. */
  key: number;
  name: string;
  /** Its name in a sort clause; `null` when the list cannot be sorted by it. */
  sortName: string | null;
  field: FieldRef | null;
}

/** The column a preference text names; `null` for an unknown text. */
export function parseColumn(text: string): Column | null {
  if ((BUILTIN_COLUMNS as readonly string[]).includes(text))
    return { kind: 'builtin', name: text as BuiltinColumn };
  const found = /^field:(\d{1,15})$/.exec(text);
  return found ? { kind: 'field', id: Number(found[1]) } : null;
}

export function columnText(column: Column): string {
  return column.kind === 'builtin' ? column.name : `field:${String(column.id)}`;
}

/** Fields a column may show: text fields are long blocks and stay on the issue page. */
export function columnFields(fields: readonly FieldRef[]): FieldRef[] {
  return fields.filter((field) => field.kind !== 'text');
}

/** The columns when the person chose none. */
export function defaultColumns(fields: readonly FieldRef[]): string[] {
  return [
    'id',
    'summary',
    ...columnFields(fields)
      .slice(0, 3)
      .map((f) => `field:${String(f.id)}`),
  ];
}

/** The chosen columns that exist, with their names; fields not in `known` are left out. */
export function shownColumns(texts: readonly string[], known: readonly FieldRef[]): ShownColumn[] {
  const byId = new Map(known.map((field) => [field.id, field]));
  const seen = new Set<string>();
  const shown: ShownColumn[] = [];
  for (const text of texts) {
    const column = parseColumn(text);
    if (!column || seen.has(text)) continue;
    seen.add(text);
    if (column.kind === 'builtin') {
      shown.push({
        column,
        text,
        key: -1 - BUILTIN_COLUMNS.indexOf(column.name),
        name: m.columnNames[column.name](),
        sortName: column.name === 'tags' ? null : column.name,
        field: null,
      });
      continue;
    }
    const field = byId.get(column.id);
    if (!field || field.kind === 'text') continue;
    shown.push({
      column,
      text,
      key: field.id,
      name: field.name,
      sortName: queryWord(field.name),
      field,
    });
  }
  return shown;
}

const cache = new Map<number | null, Promise<FieldRef[]>>();

/** The fields of the projects the viewer reads issues in, or of one project; most used first. */
export function issueFields(project: number | null): Promise<FieldRef[]> {
  let found = cache.get(project);
  if (!found) {
    found = api
      .get('/issues/fields', { query: { project: project ?? undefined } })
      .then((page) => page.items);
    found.catch(() => cache.delete(project));
    cache.set(project, found);
  }
  return found;
}

/** Forgets the loaded fields (tests; fields changed). */
export function forgetIssueFields(): void {
  cache.clear();
}
