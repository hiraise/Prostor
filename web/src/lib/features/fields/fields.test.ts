// Pure helpers of the fields screens: order changes (FLD-022, FLD-037, FLD-058), aliases (FLD-017),
// kinds (FLD-001) and filters from the address (FLD-025).
import { describe, expect, it } from 'vitest';

import {
  afterDropping,
  afterMovingDown,
  afterMovingUp,
  fieldFilters,
  mayBeMulti,
  parseAliases,
  reorder,
  setFilters,
  setKindOf,
} from './fields.ts';

describe('order', () => {
  const all = [1, 2, 3, 4];

  it('FLD-037: moves an item right after another, or first', () => {
    const key = (id: number): number => id;
    expect(reorder(all, key, 4, 1)).toEqual([1, 4, 2, 3]);
    expect(reorder(all, key, 3, null)).toEqual([3, 1, 2, 4]);
    expect(reorder(all, key, 1, 4)).toEqual([2, 3, 4, 1]);
    expect(reorder(all, key, 9, 1)).toEqual(all);
    expect(reorder(all, key, 2, 9)).toEqual(all);
  });

  it('FLD-058: Move up and Move down skip hidden items and stop at the ends', () => {
    expect(afterMovingUp(all, 3)).toBe(1);
    expect(afterMovingUp(all, 2)).toBeNull();
    expect(afterMovingUp(all, 1)).toBeUndefined();
    expect(afterMovingDown(all, 2)).toBe(3);
    expect(afterMovingDown(all, 4)).toBeUndefined();
    // 2 is hidden: moving 3 up puts it before 1.
    expect(afterMovingUp(all, 3, [1, 3, 4])).toBeNull();
    expect(afterMovingDown(all, 1, [1, 3, 4])).toBe(3);
  });

  it('FLD-022: a drop before or after a row becomes "right after"; drops that change nothing are ignored', () => {
    expect(afterDropping(all, 4, 1, 'before')).toBeNull();
    expect(afterDropping(all, 4, 2, 'before')).toBe(1);
    expect(afterDropping(all, 1, 3, 'after')).toBe(3);
    expect(afterDropping(all, 2, 1, 'after')).toBeUndefined();
    expect(afterDropping(all, 2, 3, 'before')).toBeUndefined();
    expect(afterDropping(all, 2, 2, 'after')).toBeUndefined();
    expect(afterDropping(all, 2, 9, 'after')).toBeUndefined();
  });
});

describe('fields', () => {
  it('FLD-017: aliases are split by commas, trimmed and kept once', () => {
    expect(parseAliases(' for, assigned to ,, For,x ')).toEqual(['for', 'assigned to', 'x']);
    expect(parseAliases('')).toEqual([]);
  });

  it('FLD-001: only enumerated kinds except state hold several values; sets match their kinds', () => {
    expect(mayBeMulti('enum')).toBe(true);
    expect(mayBeMulti('group')).toBe(true);
    expect(mayBeMulti('state')).toBe(false);
    expect(mayBeMulti('date')).toBe(false);
    expect(setKindOf('owned')).toBe('owned');
    expect(setKindOf('group')).toBeNull();
    expect(setKindOf('duration')).toBeNull();
  });

  it('FLD-025: filters come from the address; unknown kinds are ignored', () => {
    expect(fieldFilters(new URLSearchParams('q=+prio+&unused=true'))).toEqual({
      q: 'prio',
      unused: true,
    });
    expect(fieldFilters(new URLSearchParams(''))).toEqual({ q: '', unused: false });
    expect(setFilters(new URLSearchParams('kind=state&q=a'))).toEqual({ q: 'a', kind: 'state' });
    expect(setFilters(new URLSearchParams('kind=planet'))).toEqual({ q: '', kind: '' });
  });
});
