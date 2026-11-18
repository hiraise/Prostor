// Platform display (step 2.2 §3): the overlay and tooltips show the platform's glyphs — ⌘ on
// Apple platforms, Ctrl elsewhere — while matching stays platform-free (either physical key,
// D-063). The letter of a physical key is the letter of its position in a functional English
// layout, whatever the local layout types (§4).
import type { Combo, Sequence } from './kinds.ts';

export type Platform = 'mac' | 'pc';

export function detectPlatform(): Platform {
  if (typeof navigator === 'undefined') return 'pc';
  return /Mac|iPhone|iPad|iPod/.test(`${navigator.platform} ${navigator.userAgent}`) ? 'mac' : 'pc';
}

/** The glyph a modifier shows on a platform. */
export function modifierGlyph(modifier: 'primary' | 'alt', platform: Platform): string {
  if (modifier === 'primary') return platform === 'mac' ? '⌘' : 'Ctrl';
  return platform === 'mac' ? '⌥' : 'Alt';
}

/** The shift glyph: ⇧ on Apple platforms, the word elsewhere. */
export function shiftGlyph(platform: Platform): string {
  return platform === 'mac' ? '⇧' : 'Shift';
}

/** The letter of a physical key's position in a functional English (US) layout. */
export function letterOf(code: string): string {
  if (code.startsWith('Key')) return code.slice(3);
  if (code.startsWith('Digit')) return code.slice(5);
  const names: Record<string, string> = {
    Comma: ',',
    Period: '.',
    Slash: '/',
    Enter: 'Enter',
    Escape: 'Esc',
    Space: 'Space',
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    Home: 'Home',
    End: 'End',
  };
  return names[code] ?? code;
}

/** The chips a combo shows, in display order: modifiers, shift, the key. */
export function comboChips(combo: Combo, platform: Platform, display?: string): string[] {
  const chips: string[] = [];
  for (const modifier of combo.modifiers ?? []) chips.push(modifierGlyph(modifier, platform));
  if (combo.shift) chips.push(shiftGlyph(platform));
  chips.push(display ?? letterOf(combo.code));
  return chips;
}

/** The chips of a sequence: the leading key, then the letter — two presses. */
export function sequenceChips(sequence: Sequence): string[][] {
  return [[letterOf(sequence.first)], [letterOf(sequence.second)]];
}
