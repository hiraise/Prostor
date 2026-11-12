// The Markdown editor's formatting actions (CNT-029) as pure changes of a text and its selection.
// Each action returns one replacement, so the editor can apply it as a single typing step that
// the browser's undo takes back at once.

export type FormatAction =
  | 'heading'
  | 'bold'
  | 'italic'
  | 'strikethrough'
  | 'code'
  | 'link'
  | 'quote'
  | 'bullets'
  | 'numbers'
  | 'tasks'
  | 'codeBlock'
  | 'table';

export interface TextSelection {
  start: number;
  end: number;
}

/** Replace `from…to` with `insert`, then select `selection` (offsets in the new text). */
export interface TextEdit {
  from: number;
  to: number;
  insert: string;
  selection: TextSelection;
}

export interface FormatOptions {
  /** Header text of a new table's columns. */
  column?: string;
}

/** The text after an edit. */
export function applyEdit(text: string, edit: TextEdit): string {
  return text.slice(0, edit.from) + edit.insert + text.slice(edit.to);
}

/** The edit that `action` makes to `text` with `selection`. */
export function formatEdit(
  action: FormatAction,
  text: string,
  selection: TextSelection,
  options: FormatOptions = {},
): TextEdit {
  const start = Math.min(selection.start, selection.end);
  const end = Math.max(selection.start, selection.end);
  switch (action) {
    case 'bold':
      return wrap(text, start, end, '**');
    case 'italic':
      return wrap(text, start, end, '_');
    case 'strikethrough':
      return wrap(text, start, end, '~~');
    case 'code':
      return text.slice(start, end).includes('\n')
        ? codeBlock(text, start, end)
        : wrap(text, start, end, '`');
    case 'link':
      return link(text, start, end);
    case 'heading':
    case 'quote':
    case 'bullets':
    case 'numbers':
    case 'tasks':
      return prefixLines(text, start, end, action);
    case 'codeBlock':
      return codeBlock(text, start, end);
    case 'table':
      return table(text, start, end, options.column ?? 'Column');
  }
}

// ---- Inline marks ----

function wrap(text: string, start: number, end: number, mark: string): TextEdit {
  const selected = text.slice(start, end);
  // Already marked around the selection: take the marks away.
  if (
    start >= mark.length &&
    text.slice(start - mark.length, start) === mark &&
    text.slice(end, end + mark.length) === mark
  ) {
    const from = start - mark.length;
    return {
      from,
      to: end + mark.length,
      insert: selected,
      selection: { start: from, end: from + selected.length },
    };
  }
  // The selection includes its marks: take them away.
  if (
    selected.length >= mark.length * 2 &&
    selected.startsWith(mark) &&
    selected.endsWith(mark) &&
    selected.slice(mark.length, -mark.length).trim() !== ''
  ) {
    const inner = selected.slice(mark.length, -mark.length);
    return { from: start, to: end, insert: inner, selection: { start, end: start + inner.length } };
  }
  // Spaces at the edges of a selection stay outside the marks (`** a**` is not bold).
  const lead = selected.length - selected.trimStart().length;
  const core = selected.trim();
  const from = start + lead;
  const to = from + core.length;
  // Inline code that holds backticks needs a longer fence and spaces inside it.
  const open = mark === '`' && core.includes('`') ? '`` ' : mark;
  const close = mark === '`' && core.includes('`') ? ' ``' : mark;
  const at = from + open.length;
  return {
    from,
    to,
    insert: `${open}${core}${close}`,
    selection: { start: at, end: at + core.length },
  };
}

const URL_LIKE = /^(?:https?:\/\/|www\.)\S+$/i;

function link(text: string, start: number, end: number): TextEdit {
  const selected = text.slice(start, end).trim();
  if (URL_LIKE.test(selected)) {
    // The address is known: the caret goes where the link's text is written.
    return {
      from: start,
      to: end,
      insert: `[](${selected})`,
      selection: { start: start + 1, end: start + 1 },
    };
  }
  // The text is known: the caret goes where the address is written.
  const at = start + selected.length + 3;
  return { from: start, to: end, insert: `[${selected}]()`, selection: { start: at, end: at } };
}

// ---- Line prefixes ----

type LineAction = 'heading' | 'quote' | 'bullets' | 'numbers' | 'tasks';

const HEADING = /^#{1,6}[ \t]+/;
const QUOTE = /^>[ \t]?/;
const LIST = /^([ \t]*)(?:([-+*])|(\d{1,9})[.)])[ \t]+(\[[ xX]\][ \t]+)?/;

/** Start and end of the whole lines the selection touches. */
function lineBounds(text: string, start: number, end: number): { from: number; to: number } {
  const from = text.lastIndexOf('\n', start - 1) + 1;
  // A selection that ends at the start of a line does not take that line.
  const last = end > start && text[end - 1] === '\n' ? end - 1 : end;
  const next = text.indexOf('\n', last);
  return { from, to: Math.max(next === -1 ? text.length : next, from) };
}

function listKind(line: string): 'bullets' | 'numbers' | 'tasks' | null {
  const match = LIST.exec(line);
  if (!match) return null;
  if (match[4]) return 'tasks';
  return match[3] ? 'numbers' : 'bullets';
}

function prefixLines(text: string, start: number, end: number, action: LineAction): TextEdit {
  const { from, to } = lineBounds(text, start, end);
  const lines = text.slice(from, to).split('\n');
  const filled = lines.map((line) => line.trim() !== '');
  // Blank lines between items keep no marks, unless nothing else is selected.
  const targets = filled.some(Boolean) ? filled : lines.map(() => true);
  const has = (line: string): boolean => {
    if (action === 'heading') return HEADING.test(line);
    if (action === 'quote') return QUOTE.test(line);
    return listKind(line) === action;
  };
  const remove = lines.every((line, index) => !targets[index] || has(line));
  let number = 0;
  const changed = lines.map((line, index) => {
    if (!targets[index]) return line;
    if (action === 'heading') {
      return remove ? line.replace(HEADING, '') : `### ${line.replace(HEADING, '')}`;
    }
    if (action === 'quote') return remove ? line.replace(QUOTE, '') : `> ${line}`.trimEnd();
    const match = LIST.exec(line);
    const indent = match?.[1] ?? /^[ \t]*/.exec(line)?.[0] ?? '';
    const body = match ? line.slice(match[0].length) : line.slice(indent.length);
    if (remove) return `${indent}${body}`;
    number += 1;
    const marker =
      action === 'bullets' ? '- ' : action === 'numbers' ? `${String(number)}. ` : '- [ ] ';
    return `${indent}${marker}${body}`;
  });
  const insert = changed.join('\n');
  if (start === end && lines.length === 1) {
    // A caret stays in its place in the text it was in.
    const first = lines[0] ?? '';
    const caret = Math.max(from, start + insert.length - first.length);
    return { from, to, insert, selection: { start: caret, end: caret } };
  }
  return { from, to, insert, selection: { start: from, end: from + insert.length } };
}

// ---- Blocks ----

/** A fence longer than any run of backticks in the code. */
function fenceFor(code: string): string {
  const longest = Math.max(0, ...(code.match(/`+/g) ?? []).map((run) => run.length));
  return '`'.repeat(Math.max(3, longest + 1));
}

function codeBlock(text: string, start: number, end: number): TextEdit {
  const { from, to } = lineBounds(text, start, end);
  const code = text.slice(from, to);
  const fence = fenceFor(code);
  const at = from + fence.length + 1;
  return {
    from,
    to,
    insert: `${fence}\n${code}\n${fence}`,
    selection:
      start === end && code === '' ? { start: at, end: at } : { start: at, end: at + code.length },
  };
}

function table(text: string, start: number, end: number, column: string): TextEdit {
  const before = text.slice(0, start);
  const after = text.slice(end);
  // A table needs a blank line before it, or its first row would join the paragraph above.
  const lead =
    before === '' || before.endsWith('\n\n') ? '' : before.endsWith('\n') ? '\n' : '\n\n';
  const trail =
    after === '' || after.startsWith('\n\n') ? '' : after.startsWith('\n') ? '\n' : '\n\n';
  const rows = `| ${column} | ${column} |\n| --- | --- |\n|  |  |`;
  const at = start + lead.length + 2;
  return {
    from: start,
    to: end,
    insert: `${lead}${rows}${trail}`,
    selection: { start: at, end: at + column.length },
  };
}

// ---- The language of a code block (CNT-045) ----

const FENCE_LINE = /^ {0,3}(`{3,}|~{3,})/;

/**
 * The language being typed after an opening fence right before the caret: where it starts and the
 * word so far. A fence closes a block when an odd number of fences stands above it.
 */
export function fenceAt(text: string, caret: number): { from: number; query: string } | null {
  const lineStart = text.lastIndexOf('\n', caret - 1) + 1;
  const before = text.slice(lineStart, caret);
  const match = /^( {0,3}(?:`{3,}|~{3,}))([\w+#.-]*)$/.exec(before);
  if (!match) return null;
  const fence = match[1] ?? '';
  // Nothing may follow the caret on the line.
  const lineEnd = text.indexOf('\n', caret);
  if (text.slice(caret, lineEnd < 0 ? text.length : lineEnd).trim() !== '') return null;
  const above = text
    .slice(0, lineStart)
    .split('\n')
    .filter((line) => FENCE_LINE.test(line));
  if (above.length % 2 === 1) return null;
  return { from: lineStart + fence.length, query: match[2] ?? '' };
}
