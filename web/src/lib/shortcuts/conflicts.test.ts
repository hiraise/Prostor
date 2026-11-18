// The conflict rules (step 2.2 §3): the binding table is data, so its invariants are a unit
// test `make web-test` runs — our «build-time» conflict detection, gated by `make check`.
import { describe, expect, it } from 'vitest';

import type { Binding, ContextId } from './kinds.ts';
import { bindingGroups, globalGroups, labels } from './messages.ts';
import { contextIds, tables } from './tables/index.ts';

type AnyContext = 'global' | ContextId;

function comboKey(binding: Binding): string {
  const combo = binding.combo;
  if ('first' in combo) return `${combo.first} ${combo.second}`;
  return [
    combo.code,
    ...[...(combo.modifiers ?? [])].sort(),
    combo.shift ? 'shift' : 'no-shift',
  ].join('+');
}

/** A single-key combo: no real modifier — the kind the typing guard is about. Shift alone still
 * types a character, so it counts. */
function isSingleKey(binding: Binding): boolean {
  const combo = binding.combo;
  if ('first' in combo) return true;
  return (combo.modifiers?.length ?? 0) === 0;
}

const allContexts: readonly AnyContext[] = ['global', ...contextIds];

describe('binding tables', () => {
  it('gives every binding a unique id', () => {
    const ids = allContexts.flatMap((context) => tables[context].map((binding) => binding.id));
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('keeps one combo to a binding within a context', () => {
    for (const context of allContexts) {
      const keys = tables[context].map(comboKey);
      expect(new Set(keys).size, context).toBe(keys.length);
    }
  });

  it('keeps global single keys clear of the single keys of every context', () => {
    const globals = new Set(tables.global.filter(isSingleKey).map(comboKey));
    for (const context of contextIds) {
      for (const binding of tables[context].filter(isSingleKey)) {
        expect(globals.has(comboKey(binding)), `${context} ${binding.id}`).toBe(false);
      }
    }
  });

  it('starts every sequence with g', () => {
    for (const context of allContexts) {
      for (const binding of tables[context]) {
        if (!('first' in binding.combo)) continue;
        expect(binding.combo.first, binding.id).toBe('KeyG');
      }
    }
  });

  it('labels every binding and gives it a group', () => {
    for (const context of allContexts) {
      for (const binding of tables[context]) {
        expect(labels, `label of ${binding.id}`).toHaveProperty(binding.id);
        if (context === 'global') {
          expect(globalGroups, `group of ${binding.id}`).toHaveProperty(binding.id);
        } else {
          expect(bindingGroups[context], `group of ${binding.id}`).toHaveProperty(binding.id);
        }
      }
    }
  });
});
