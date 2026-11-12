// Ticking a checklist item of a rendered text (CNT-041): the server marks every list item with the
// line it starts on (`data-sourcepos`), so the box changes exactly that line of the Markdown.

/** A list item that holds a box: quote markers, the list marker, then `[ ]` or `[x]`. */
const TASK = /^((?:[ \t]*>)*[ \t]*(?:[-+*]|\d{1,9}[.)])[ \t]+\[)([ xX])(\])/;

/**
 * The text with the box on `line` (1-based) ticked or cleared, or `null` when that line holds no
 * box any more (the text changed meanwhile).
 */
export function toggleTask(source: string, line: number, checked: boolean): string | null {
  if (!Number.isInteger(line) || line < 1) return null;
  const lines = source.split('\n');
  const text = lines[line - 1];
  if (text === undefined) return null;
  const match = TASK.exec(text);
  if (!match) return null;
  const [whole, before = '', , after = ''] = match;
  lines[line - 1] = `${before}${checked ? 'x' : ' '}${after}${text.slice(whole.length)}`;
  return lines.join('\n');
}

/** The line a rendered list item starts on, from its `data-sourcepos="L:C-L:C"`. */
export function sourceLine(position: string | null | undefined): number | null {
  const match = /^(\d+):\d+-/.exec(position ?? '');
  return match ? Number(match[1]) : null;
}
