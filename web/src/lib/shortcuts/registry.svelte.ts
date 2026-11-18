// The context stack and the page hooks of the shortcut framework (step 2.2 §3, UI-024). Pages
// push their context on mount and may say what «the focused entity» is and where the page's
// query box sits; nobody listens for app shortcuts themselves — the shell holds the one keydown
// listener and resolves the active context through this registry, not by stopPropagation.
import { untrack } from 'svelte';

import type { ContextId } from './kinds.ts';

/** What the global copy trio (§4) needs of the focused entity: the focused row's issue, the open
 * issue — whatever the page says is in focus. */
export interface FocusedEntity {
  /** What «copy the ID» copies, e.g. `CORE-12`. */
  id: string;
  /** The entity's address in the app, without the origin. */
  href: string;
  /** The entity's title, for «ID with summary». */
  summary: string;
}

let stack = $state<ContextId[]>([]);

/** A page enters its context on mount; the innermost context is the focused one. The change is
 * untracked so a caller's effect does not make the stack its own dependency — an effect that
 * reads and writes it would never settle (part 2's note). */
export function enter(id: ContextId): void {
  untrack(() => {
    stack.push(id);
  });
}

/** Leaves the innermost occurrence; call from the effect's cleanup. */
export function leave(id: ContextId): void {
  untrack(() => {
    const at = stack.lastIndexOf(id);
    if (at !== -1) stack.splice(at, 1);
  });
}

/** The focused context, or null when only global bindings are active. Reactive. */
export function active(): ContextId | null {
  return stack.at(-1) ?? null;
}

// The hooks are single-slot: one page is mounted at a time, and the last page to register owns
// the hook until its cleanup runs.

let entityGetter: (() => FocusedEntity | null) | null = null;

/** Says what the focused entity is; `null` clears it. Returns nothing — pair with a `$effect`
 * cleanup that clears, or call again with the next getter. */
export function setFocusedEntity(getter: (() => FocusedEntity | null) | null): void {
  entityGetter = getter;
}

/** The focused entity of the page, or null when the page has none. */
export function focusedEntity(): FocusedEntity | null {
  return entityGetter?.() ?? null;
}

/** The page's query boxes: `/` focuses the last visible one (a board's filter counts; a closed
 * panel's bar does not, it answers false). */
const queryFocuses: (() => boolean)[] = [];

/** Where `/` focuses: the page's query box registers itself (part 1's focus hook). Returns the
 * release. */
export function setQueryFocus(focus: () => boolean): () => void {
  queryFocuses.push(focus);
  return () => {
    const at = queryFocuses.indexOf(focus);
    if (at !== -1) queryFocuses.splice(at, 1);
  };
}

/** Focuses the page's query box; false when the page has none (the caller goes to «Задачи»). */
export function focusQuery(): boolean {
  for (let i = queryFocuses.length - 1; i >= 0; i -= 1) {
    if (queryFocuses[i]?.()) return true;
  }
  return false;
}

/** The claimed actions, by binding id; no reactivity needed — the shell asks on each press. An
 * answer of `false` refuses without acting; anything else takes the key. */
export type ClaimedAction = (preset?: string) => boolean | undefined;

const claims: Record<string, ClaimedAction | undefined> = {};

/** Claims a binding's action for the page that owns its subject — the command dialog for the
 * open or selected items (§4). `preset` is what the dialog opens with: the letter a
 * `commands_on_typing` press typed. A `false` answer refuses without acting. Returns the
 * release. */
export function claim(id: string, handler: ClaimedAction): () => void {
  claims[id] = handler;
  return () => {
    if (claims[id] === handler) claims[id] = undefined;
  };
}

/** Runs a claimed action; false when no page owns it or the page refused. */
export function dispatchClaim(id: string, preset?: string): boolean {
  const handler = claims[id];
  if (!handler) return false;
  return handler(preset) !== false;
}

/** Clears the registry between component tests. */
export function resetShortcutsForTests(): void {
  stack = [];
  entityGetter = null;
  queryFocuses.length = 0;
  for (const id of Object.keys(claims)) claims[id] = undefined;
}
