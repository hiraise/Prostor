// Sorting by a column header (QRY-075) and the columns of the list (QRY-078), without a browser.
import { describe, expect, it } from 'vitest';

import type { FieldRef } from '#lib/api/schema.gen.ts';

import { defaultColumns, parseColumn, shownColumns } from './columns.ts';
import { nextDirection, sortedBy, sortedQuery } from './sort.ts';

const PRIORITY: FieldRef = {
  id: 11,
  name: 'Priority',
  kind: 'enum',
  multi: false,
  builtin: null,
  can_be_empty: false,
  empty_label: '',
};
const FIX: FieldRef = {
  id: 12,
  name: 'Fix version',
  kind: 'version',
  multi: true,
  builtin: null,
  can_be_empty: true,
  empty_label: 'No version',
};
const NOTES: FieldRef = {
  id: 15,
  name: 'Notes',
  kind: 'text',
  multi: false,
  builtin: null,
  can_be_empty: true,
  empty_label: 'No notes',
};

describe('QRY-075: the sort clause of a header click', () => {
  it('replaces the clause the language found', () => {
    const q = 'login sort: updated desc #open';
    expect(sortedQuery(q, { from: 6, to: 24 }, 'summary', 'asc')).toBe(
      'login sort: summary asc #open',
    );
  });

  it('appends a clause to a query without one, or starts one', () => {
    expect(sortedQuery('login  ', null, 'votes', 'desc')).toBe('login sort: votes desc');
    expect(sortedQuery('', null, 'votes', 'asc')).toBe('sort: votes asc');
  });

  it('goes ascending first, then descending', () => {
    expect(nextDirection(null)).toBe('asc');
    expect(nextDirection('asc')).toBe('desc');
    expect(nextDirection('desc')).toBe('asc');
  });

  it('reads the order in use from the first key the server reports', () => {
    const sort = [{ key: 'field' as const, field: 11, direction: 'desc' as const }];
    expect(sortedBy({ kind: 'field', id: 11 }, sort)).toBe('desc');
    expect(sortedBy({ kind: 'field', id: 12 }, sort)).toBeNull();
    expect(
      sortedBy({ kind: 'builtin', name: 'resolved' }, [
        { key: 'resolved', field: null, direction: 'asc' },
      ]),
    ).toBe('asc');
    expect(sortedBy({ kind: 'builtin', name: 'id' }, [])).toBeNull();
  });
});

describe('QRY-078: columns', () => {
  it('reads preference texts and leaves unknown ones out', () => {
    expect(parseColumn('votes')).toEqual({ kind: 'builtin', name: 'votes' });
    expect(parseColumn('field:12')).toEqual({ kind: 'field', id: 12 });
    expect(parseColumn('field:x')).toBeNull();
    const shown = shownColumns(
      ['id', 'field:99', 'field:12', 'field:15', 'tags', 'id'],
      [FIX, NOTES],
    );
    expect(shown.map((c) => c.text)).toEqual(['id', 'field:12', 'tags']);
    expect(shown.map((c) => c.sortName)).toEqual(['id', '"Fix version"', null]);
  });

  it('shows ID, summary and three fields, never a text field, by default', () => {
    expect(defaultColumns([NOTES, PRIORITY, FIX])).toEqual([
      'id',
      'summary',
      'field:11',
      'field:12',
    ]);
  });
});
