// The kinds of the shortcut framework (step 2.2 §3, UI-019, UI-024). A binding is one row of the
// binding table (§4); tables are static data, one module per context in `tables/`.

/** The page contexts beside `global` (§3). At most one is focused at a time. */
export type ContextId = 'list' | 'issue' | 'editor' | 'dialog' | 'query';

/**
 * The modifiers a combo needs. `primary` is ⌘ on Apple platforms and Ctrl elsewhere: matching
 * accepts either physical key, so one binding serves every platform, and only the display picks
 * the glyph (D-063). `alt` is Option on Apple platforms.
 */
export type Modifier = 'primary' | 'alt';

/** A physical key with modifiers: `code` is `KeyboardEvent.code` of a functional English (US)
 * layout, never `event.key` — on another layout the key at that position acts (§4). */
export interface Combo {
  code: string;
  modifiers?: readonly Modifier[];
  shift?: boolean;
}

/** A two-key sequence (§3): a leading key plus one letter — only `g` exists today. */
export interface Sequence {
  first: string;
  second: string;
}

/**
 * One row of the binding table. `id` names the action for the wiring and the overlay label;
 * `contexts` is `global` (active everywhere) or the one context the binding lives in; `if` gates
 * the binding — it neither fires nor shows in the overlay while it does not hold.
 */
export interface Binding {
  id: string;
  contexts: readonly ('global' | ContextId)[];
  combo: Combo | Sequence;
  /** Replaces the glyph of the combo's key in display (rare). */
  display?: string;
  if?: () => boolean;
}

/** The tables a key press resolves over: global plus the focused context. */
export type Tables = Readonly<Record<'global' | ContextId, readonly Binding[]>>;
