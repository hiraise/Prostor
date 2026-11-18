// The DOM twin of the typing guard (step 2.2 §3): whether the key press would land in a text
// field. One helper for the shell's listener, the pages' handlers and the editors — the guard
// itself lives in `match.ts`, pure of the DOM.

/** True while an input, textarea, select or contenteditable field has the focus. */
export function isTypingFocus(): boolean {
  return (
    document.activeElement instanceof HTMLElement &&
    !!document.activeElement.closest('input, textarea, select, [contenteditable="true"]')
  );
}
