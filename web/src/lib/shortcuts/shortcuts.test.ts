// Matching and display (UI-019, step 2.2 §3–§4): the context resolution, the typing guard, the
// either-key primary modifier, sequences with their window, and the platform glyphs. Plain
// objects stand for keyboard events — match() is pure of the DOM.
import { describe, expect, it } from 'vitest';

import { setAccessForTests } from '#lib/session/index.ts';

import type { Binding, Tables } from './kinds.ts';
import { createMatchState, match, type ShortcutEvent } from './match.ts';
import { comboChips, letterOf, modifierGlyph, shiftGlyph } from './platform.ts';
import { tables } from './tables/index.ts';

function key(
  code: string,
  at = 0,
  over: Partial<Omit<ShortcutEvent, 'code' | 'timeStamp'>> = {},
): ShortcutEvent {
  return {
    code,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    timeStamp: at,
    ...over,
  };
}

const state = (typing = false, time = { now: 0 }) => ({
  ...createMatchState(
    () => typing,
    () => time.now,
  ),
  tick(ms: number): void {
    time.now += ms;
  },
});

describe('match', () => {
  it('resolves global bindings with either primary key', () => {
    setAccessForTests({ global: ['issue.create'], projects: [], capabilities: [] });
    const s = state();
    expect(match(key('KeyC', 0, { metaKey: true, altKey: true }), tables, null, s)?.id).toBe(
      'command',
    );
    expect(match(key('KeyC', 0, { ctrlKey: true, altKey: true }), tables, null, s)?.id).toBe(
      'command',
    );
    // The create menu is the plain letter; Ctrl without Alt fires nothing.
    expect(match(key('KeyC'), tables, null, s)?.id).toBe('create');
    expect(match(key('KeyC', 0, { ctrlKey: true }), tables, null, s)).toBeNull();
    setAccessForTests(null);
    // The create binding is permission-gated: without the right it is gone.
    expect(match(key('KeyC'), tables, null, s)).toBeNull();
  });

  it('matches the focused context beside global, exactly on shift', () => {
    const s = state();
    expect(match(key('KeyX'), tables, 'list', s)?.id).toBe('list.select');
    expect(match(key('KeyY'), tables, 'list', s)?.id).toBe('copy.link');
    expect(match(key('KeyY', 0, { shiftKey: true }), tables, 'list', s)?.id).toBe('copy.full');
    expect(match(key('KeyX'), tables, null, s)).toBeNull();
    // Not the only modifier set: a stray Shift keeps a plain letter from firing.
    expect(match(key('KeyX', 0, { shiftKey: true }), tables, 'list', s)).toBeNull();
  });

  it('keeps single-key bindings from firing while typing; modifier combos still may', () => {
    const s = state(true);
    expect(match(key('KeyX'), tables, 'list', s)).toBeNull();
    expect(match(key('Slash', 0, { shiftKey: true }), tables, null, s)).toBeNull();
    expect(match(key('KeyC', 0, { metaKey: true, altKey: true }), tables, null, s)?.id).toBe(
      'command',
    );
  });

  it('inverts the guard in the text contexts: their keys keep working in the field', () => {
    const s = state(true);
    expect(match(key('Enter'), tables, 'dialog', s)?.id).toBe('dialog.apply');
    expect(match(key('Escape'), tables, 'query', s)?.id).toBe('query.leave');
    expect(match(key('KeyB', 0, { metaKey: true }), tables, 'editor', s)?.id).toBe('editor.bold');
    // The global letters stay suppressed even there.
    expect(match(key('KeyC'), tables, 'query', s)).toBeNull();
  });

  it('walks sequences: g arms a 1 s window for one letter', () => {
    const s = state();
    expect(match(key('KeyG', 0), tables, null, s)).toBeNull();
    expect(match(key('KeyI', 10), tables, null, s)?.id).toBe('go.issues');
    // A second g re-arms instead of completing.
    expect(match(key('KeyG', 20), tables, null, s)).toBeNull();
    expect(match(key('KeyG', 30), tables, null, s)).toBeNull();
    expect(match(key('KeyB', 40), tables, null, s)?.id).toBe('go.boards');
    // The window closes: a late letter is just a letter.
    expect(match(key('KeyG', 50), tables, null, s)).toBeNull();
    s.tick(1001);
    expect(match(key('KeyI', 1051), tables, null, s)?.id).toBe('copy.id');
  });

  it('lets a non-sequence key after g take its own meaning', () => {
    setAccessForTests({ global: ['issue.create'], projects: [], capabilities: [] });
    const s = state();
    expect(match(key('KeyG', 0), tables, null, s)).toBeNull();
    // c is no sequence's letter, so the create menu fires.
    expect(match(key('KeyC', 5), tables, null, s)?.id).toBe('create');
    setAccessForTests(null);
  });

  it('never arms a sequence while typing or with modifiers held', () => {
    const typing = state(true);
    expect(match(key('KeyG', 0), tables, null, typing)).toBeNull();
    // No window opened, and the typing guard still holds the letter down.
    expect(match(key('KeyI', 5), tables, null, typing)).toBeNull();
    const held = state();
    expect(match(key('KeyG', 0, { shiftKey: true }), tables, null, held)).toBeNull();
    expect(match(key('KeyI', 5), tables, null, held)?.id).toBe('copy.id');
  });

  it('honours the binding conditions', () => {
    const allowed = { value: false };
    const gated: Binding = {
      id: 'test.gated',
      contexts: ['global'],
      combo: { first: 'KeyG', second: 'KeyA' },
      if: () => allowed.value,
    };
    const withGate: Tables = { ...tables, global: [...tables.global, gated] };
    const s = state();
    expect(match(key('KeyG', 0), withGate, null, s)).toBeNull();
    expect(match(key('KeyA', 5), withGate, null, s)).toBeNull();
    allowed.value = true;
    expect(match(key('KeyG', 10), withGate, null, s)).toBeNull();
    expect(match(key('KeyA', 15), withGate, null, s)?.id).toBe('test.gated');
  });
});

describe('platform display', () => {
  it('glyphs the primary modifier per platform', () => {
    expect(modifierGlyph('primary', 'mac')).toBe('⌘');
    expect(modifierGlyph('primary', 'pc')).toBe('Ctrl');
    expect(modifierGlyph('alt', 'mac')).toBe('⌥');
    expect(shiftGlyph('pc')).toBe('Shift');
  });

  it('builds combo chips in display order', () => {
    const command = tables.global.find((binding) => binding.id === 'command');
    expect(command && 'code' in command.combo ? comboChips(command.combo, 'pc') : null).toEqual([
      'Ctrl',
      'Alt',
      'C',
    ]);
    const full = tables.global.find((binding) => binding.id === 'copy.full');
    expect(full && 'code' in full.combo ? comboChips(full.combo, 'mac') : null).toEqual(['⇧', 'Y']);
  });

  it('names the physical keys of a functional English layout', () => {
    expect(letterOf('KeyQ')).toBe('Q');
    expect(letterOf('Digit7')).toBe('7');
    expect(letterOf('Slash')).toBe('/');
    expect(letterOf('ArrowUp')).toBe('↑');
    expect(letterOf('Escape')).toBe('Esc');
  });
});
