// The completion popup of the command dialog (plan 2.1 §6, CMD-029): a pure model over the
// assist's suggestions — which one is active, what an insert does to the text — so the component
// stays markup and keys. Offsets of suggestions are UTF-16 code units, as JavaScript counts
// them; problems and operations arrive in bytes (plan §4.8) and convert here.
import type { Suggestion } from '#lib/api/schema.gen.ts';

/** The popup's shape the component keeps. */
export interface Completion {
  /** The active row, when one is chosen; `null` is no row (fresh list: the first is active once the list is open). */
  index: number | null;
}

/** Moves the active row by `delta`, wrapping; `null` picks the first (or last for up).
 * Returns `null` when there is nothing to move in. */
export function move(active: number | null, delta: number, count: number): number | null {
  if (count === 0) return null;
  if (active === null) return delta > 0 ? 0 : count - 1;
  return (active + delta + count) % count;
}

/** What inserting one suggestion does: the new text and where the caret lands. */
export function inserted(text: string, suggestion: Suggestion): { text: string; caret: number } {
  const next =
    text.slice(0, suggestion.from) +
    suggestion.text +
    suggestion.suffix +
    text.slice(suggestion.to);
  return { text: next, caret: suggestion.caret };
}

/** A byte offset of the command text, as the API counts it (UTF-8), in UTF-16 code units, as
 * the input counts them. `text.length` when the offset runs past (a range the text no longer
 * has). */
export function utf16At(text: string, byteOffset: number): number {
  let bytes = 0;
  let units = 0;
  for (const char of text) {
    if (bytes >= byteOffset) return units;
    const point = char.codePointAt(0) ?? 0;
    bytes += point < 0x80 ? 1 : point < 0x800 ? 2 : point < 0x10000 ? 3 : 4;
    units += char.length;
  }
  return units;
}

/** Whether the key the input received belongs to the popup: arrows, Tab and the right arrow's
 * insert. Enter is not here: it applies the command (plan §6). */
export function popupKey(event: KeyboardEvent): 'up' | 'down' | 'insert' | null {
  if (event.altKey || event.ctrlKey || event.metaKey) return null;
  if (event.key === 'ArrowDown') return 'down';
  if (event.key === 'ArrowUp') return 'up';
  if (event.key === 'Tab' || (event.key === 'ArrowRight' && !event.shiftKey)) return 'insert';
  return null;
}
