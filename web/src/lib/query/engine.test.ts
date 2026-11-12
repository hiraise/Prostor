// The browser's copy of the language answers like the server's: the shared cases of
// `crates/query/tests/cases.json` (offsets there are UTF-16, as here).

import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { engineFrom } from './engine.ts';
import type { QueryEngine } from './types.ts';

interface Case {
  name: string;
  text: string;
  print?: string;
  problems?: [string, number, number][];
  classes?: [number, number, string][];
}

const root = new URL('../../../../', import.meta.url);
const cases = JSON.parse(
  readFileSync(new URL('crates/query/tests/cases.json', root), 'utf8'),
) as Record<string, Case[]>;

async function engine(): Promise<QueryEngine> {
  const bytes = readFileSync(new URL('./query.wasm', import.meta.url));
  const { instance } = await WebAssembly.instantiate(bytes);
  return engineFrom(instance);
}

describe('query engine', () => {
  it('QRY-001: answers every shared case like the Rust crate', async () => {
    const e = await engine();
    let checked = 0;
    for (const [group, list] of Object.entries(cases)) {
      for (const c of list) {
        const found = e.analyze(c.text);
        const label = `${group}: ${c.name}`;
        expect(
          found.problems.map((p) => [p.code, p.from, p.to]),
          label,
        ).toEqual(c.problems ?? []);
        if (c.classes) {
          expect(
            found.tokens.map((t) => [t.from, t.to, t.class]),
            label,
          ).toEqual(c.classes);
        }
        if (c.print !== undefined) expect(found.canonical, label).toBe(c.print);
        checked += 1;
      }
    }
    expect(checked).toBeGreaterThan(50);
  });

  it('QRY-062: counts UTF-16 units across Cyrillic and emoji', async () => {
    const e = await engine();
    const found = e.analyze('тег:  🎉x (');
    expect(found.tokens[0]).toEqual({ from: 0, to: 3, class: 'attribute' });
    expect(found.tokens.find((t) => t.class === 'value')).toMatchObject({ from: 6, to: 9 });
    expect(found.problems).toEqual([
      { from: 10, to: 11, code: 'unclosed_group', severity: 'error' },
    ]);
  });

  it('QRY-075: places the sort clause for a header click', async () => {
    const e = await engine();
    const text = 'тег: 🎉 sort: id desc';
    const found = e.analyze(text);
    expect(found.sort).not.toBeNull();
    expect(text.slice(found.sort?.from, found.sort?.to)).toBe('sort: id desc');
    expect(e.analyze('is: open').sort).toBeNull();
  });

  it('survives many calls and a long query (memory grows, nothing leaks into answers)', async () => {
    const e = await engine();
    const long = Array.from({ length: 300 }, (_, i) => `f${String(i)}: v${String(i)}`).join(' or ');
    for (let i = 0; i < 200; i += 1) e.analyze(long);
    expect(e.analyze('').canonical).toBe('');
    expect(e.analyze('is: open').canonical).toBe('is: open');
  });
});
