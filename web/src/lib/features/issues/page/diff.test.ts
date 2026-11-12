// The word diff of the conflict view (ISS-051); the emails of notifications show the same diff
// (NTF-060), so both sides run the cases of crates/domain/src/diff_cases.json.
import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { diffWords, MAX_TOKENS, type DiffPart } from './diff.ts';

interface DiffCase {
  name: string;
  before: string;
  after: string;
  parts: [DiffPart['op'], string][];
}

const sharedCases: DiffCase[] = JSON.parse(
  readFileSync(
    new URL('../../../../../../crates/domain/src/diff_cases.json', import.meta.url),
    'utf8',
  ),
) as DiffCase[];

/** The texts a diff puts back together. */
function sides(parts: DiffPart[]): [string, string] {
  const before = parts.filter((part) => part.op !== 'added').map((part) => part.text);
  const after = parts.filter((part) => part.op !== 'removed').map((part) => part.text);
  return [before.join(''), after.join('')];
}

describe('ISS-051: diffWords', () => {
  it('marks a changed word and keeps the rest', () => {
    expect(diffWords('The login button does nothing', 'The submit button does nothing')).toEqual([
      { op: 'same', text: 'The ' },
      { op: 'removed', text: 'login' },
      { op: 'added', text: 'submit' },
      { op: 'same', text: ' button does nothing' },
    ]);
  });

  it('finds additions and removals in the middle', () => {
    expect(diffWords('a b c d', 'a x b d')).toEqual([
      { op: 'same', text: 'a ' },
      { op: 'removed', text: 'b c' },
      { op: 'added', text: 'x b' },
      { op: 'same', text: ' d' },
    ]);
    expect(diffWords('one two three', 'one three')).toEqual([
      { op: 'same', text: 'one ' },
      { op: 'removed', text: 'two ' },
      { op: 'same', text: 'three' },
    ]);
  });

  it('handles empty and equal texts', () => {
    expect(diffWords('', '')).toEqual([]);
    expect(diffWords('same', 'same')).toEqual([{ op: 'same', text: 'same' }]);
    expect(diffWords('', 'new text')).toEqual([{ op: 'added', text: 'new text' }]);
    expect(diffWords('old', '')).toEqual([{ op: 'removed', text: 'old' }]);
  });

  it('always rebuilds both texts', () => {
    const cases: [string, string][] = [
      ['one two three\nfour', 'two three four\n\nfive'],
      ['x y z', 'z y x'],
      ['  lead and trail  ', 'lead  and trail'],
      ['Пишет кириллицей', 'Пишет латиницей и кириллицей'],
    ];
    for (const [before, after] of cases) {
      expect(sides(diffWords(before, after))).toEqual([before, after]);
    }
  });

  it('gives very long texts as one removal and one addition', () => {
    const long = Array.from({ length: MAX_TOKENS }, (_, index) => `w${String(index)}`).join(' ');
    expect(diffWords(long, `${long} more`).map((part) => part.op)).toEqual(['removed', 'added']);
  });

  it('gives very different texts whole', () => {
    const before = Array.from({ length: 1500 }, (_, index) => `a${String(index)}`).join(' ');
    const after = Array.from({ length: 1500 }, (_, index) => `b${String(index)}`).join(' ');
    const parts = diffWords(before, after);
    expect(sides(parts)).toEqual([before, after]);
    expect(parts.length).toBeLessThanOrEqual(3);
  });

  it.each(sharedCases)('NTF-060 shared case: $name', ({ before, after, parts }) => {
    expect(diffWords(before, after)).toEqual(parts.map(([op, text]) => ({ op, text })));
  });
});
