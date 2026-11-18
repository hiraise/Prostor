// Matching (step 2.2 §3, UI-019): the one function the shell's single keydown listener calls. It
// resolves global plus the focused context, applies the typing guard, walks sequences, and
// returns the winning binding or null. Pure of the DOM — the caller says whether a text field
// has focus and provides the clock, so unit tests build plain event objects.
import type { Binding, Combo, ContextId, Tables } from './kinds.ts';

/** What match() needs of a keyboard event, so tests build plain objects in node. */
export interface ShortcutEvent {
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  timeStamp: number;
}

/** The mutable state of one listener: the armed sequence and the environment it decides by. */
export interface MatchState {
  /** The armed leading key and when it was pressed (ms). */
  armed: { code: string; at: number } | null;
  /** Whether a text field (input, textarea, select, contenteditable) has focus. */
  isTyping(): boolean;
  /** The clock the 1 s sequence window expires by. */
  now(): number;
}

/** The state for the shell's listener; tests build their own with a fixed clock. */
export function createMatchState(
  isTyping: () => boolean,
  now: () => number = () => performance.now(),
): MatchState {
  return { armed: null, isTyping, now };
}

/** How long a sequence's leading key waits for its letter (§3). */
export const SEQUENCE_WINDOW = 1000;

/** The contexts whose own field holds the keys: the guard inverts there — letters go to the
 * text, and the table's Enter, Escape and modifier combos keep acting (§3). */
const TYPING_CONTEXTS: readonly ContextId[] = ['editor', 'dialog', 'query'];

/**
 * Finds the binding the event fires, or null when it fires none.
 *
 * The focused context's bindings win over global ones with the same combo (the context is the
 * more specific answer); the conflict test keeps both sides from colliding anyway.
 */
export function match(
  event: ShortcutEvent,
  tables: Tables,
  context: ContextId | null,
  state: MatchState,
): Binding | null {
  const candidates = [...(context !== null ? tables[context] : []), ...tables.global].filter(
    (binding) => binding.if?.() ?? true,
  );

  // Sequences: an unmodified leading-key press arms the window; the next key either completes a
  // sequence or disarms and takes its own chance through the combos.
  const lead = sequenceLead(event, state);
  if (lead === 'arm') {
    state.armed = { code: event.code, at: state.now() };
    return null;
  }
  let sequenceHit: Binding | null = null;
  if (lead === 'consume') sequenceHit = completeSequence(event, candidates, state);
  else if (lead === 'disarm') state.armed = null;
  if (sequenceHit !== null) return sequenceHit;

  for (const binding of candidates) {
    if ('first' in binding.combo) continue;
    if (comboMatches(binding.combo, event) && guardPasses(binding, state)) return binding;
  }
  return null;
}

/** What the event means for the armed sequence: arm, consume as the second key, disarm and move
 * on, or nothing (no sequence is in play). */
function sequenceLead(
  event: ShortcutEvent,
  state: MatchState,
): 'arm' | 'consume' | 'disarm' | null {
  const unmodified = !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey;
  if (state.armed !== null && state.now() - state.armed.at <= SEQUENCE_WINDOW) {
    if (unmodified) return 'consume';
    // A modified press is not a sequence's second key; it keeps its own meaning.
    return 'disarm';
  }
  state.armed = null;
  if (unmodified && !state.isTyping() && event.code === 'KeyG') return 'arm';
  return null;
}

/** The armed window's second key: a matching sequence wins, anything else disarms and the key
 * falls through to the combos below. */
function completeSequence(
  event: ShortcutEvent,
  candidates: readonly Binding[],
  state: MatchState,
): Binding | null {
  const armed = state.armed;
  state.armed = null;
  if (armed === null) return null;
  if (event.code === armed.code) {
    // `g g …`: the second press re-arms instead of completing.
    state.armed = { code: armed.code, at: state.now() };
    return null;
  }
  for (const binding of candidates) {
    if (!('first' in binding.combo)) continue;
    if (binding.combo.first === armed.code && binding.combo.second === event.code) return binding;
  }
  return null;
}

/** Exact match on the physical key and its modifiers: `primary` is ⌘ or Ctrl (either physical
 * key, D-063), `alt` is Alt or Option, `shift` must be exactly as the combo says. */
function comboMatches(combo: Combo, event: ShortcutEvent): boolean {
  const primary = event.metaKey || event.ctrlKey;
  const modifiers = combo.modifiers ?? [];
  if (modifiers.includes('primary') !== primary) return false;
  if (modifiers.includes('alt') !== event.altKey) return false;
  if ((combo.shift ?? false) !== event.shiftKey) return false;
  return combo.code === event.code;
}

/**
 * The typing guard (§3): while a text field has focus, single-key bindings never fire and
 * modifier combos always may. Escape is exempt — it never types a character. The text contexts
 * invert the guard for their own tables, which hold no letter keys by construction.
 */
function guardPasses(binding: Binding, state: MatchState): boolean {
  if (!state.isTyping()) return true;
  const own = binding.contexts.find((context) => context !== 'global');
  if (own !== undefined && TYPING_CONTEXTS.includes(own)) return true;
  if ('first' in binding.combo) return false;
  const combo = binding.combo;
  if ((combo.modifiers?.length ?? 0) > 0) return true;
  return combo.code === 'Escape';
}
