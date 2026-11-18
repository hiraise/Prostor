// The keys of the list's rows (step 2.2 §6, QRY-085, UI-022): one handler the table's rows and the
// phone cards share. The combos come from the binding table (§4) — the framework's match runs
// over the list's context only, so global keys bubble on to the shell's listener untouched. The
// selection keys (`x`, `Shift+↑/↓`, `Ctrl/⌘+A`, §8) drive the selection model of §8.1.
import type { Issue } from '#lib/api/schema.gen.ts';
import { createMatchState, match, type MatchState } from '#lib/shortcuts/match.ts';
import { focusQuery } from '#lib/shortcuts/registry.svelte.ts';
import { tables } from '#lib/shortcuts/tables/index.ts';
import { isTypingFocus } from '#lib/shortcuts/typing.ts';

/** The list's bindings alone: `match` resolves the active context plus global, so global is
 * emptied here — the shell runs those, and the row never outvotes it. */
const ROW_TABLES = { ...tables, global: [] };

/** What the shared handler needs of the list: the rows, the roving focus and the row's actions —
 * the same ones `RowMenu` and `RowStar` call (§6). */
export interface RowKeysDeps {
  /** The rows shown, in order. */
  rows(): readonly Issue[];
  /** Moves the roving focus to a row, or `null` for the query bar; focuses the row's element and
   * announces it. */
  focus(id: number | null): void;
  open(issue: Issue, newTab: boolean): void;
  edit(issue: Issue): void;
  star(issue: Issue): void;
  assignMe(issue: Issue): void;
  sidebar(): void;
  /** One page back (`-1`) or forward (`1`) through the loaded rows. */
  page(step: 1 | -1): void;
  /** The selection (§8.1): `x` toggles the row, `Shift+↑/↓` extends the range, `Ctrl/⌘+A`
   * chooses the page. */
  select: {
    toggle(issue: Issue): void;
    extend(issue: Issue, step: 1 | -1): void;
    page(): void;
  };
}

/** The row-level keydown the table's rows and the phone cards share. */
export type RowKeydown = (event: KeyboardEvent, issue: Issue) => void;

/** Builds the handler; one instance per list holds the sequence state. */
export function createRowKeys(deps: RowKeysDeps): RowKeydown {
  const state: MatchState = createMatchState(isTypingFocus);
  return (event: KeyboardEvent, issue: Issue): void => {
    // The bail conditions of the shell's listener (§3): a page's earlier handler and an open
    // dialog or popover (a cell's editor, a menu) own the key.
    if (event.defaultPrevented) return;
    if (document.querySelector('dialog[open], :popover-open')) return;
    // Esc leaves the list for the query bar (§6) — focus management, not a binding: the roving
    // focus clears, so Tab enters the list again at the first row.
    if (event.code === 'Escape') {
      deps.focus(null);
      if (focusQuery()) event.preventDefault();
      return;
    }
    // Enter keeps its native meaning on an activatable element of the row (the star, a cell's
    // editor button, a link): the binding does not outvote a click.
    if (
      event.code === 'Enter' &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.shiftKey &&
      event.target instanceof HTMLElement &&
      event.target.closest('button, a[href], input, select, textarea') !== null
    ) {
      return;
    }
    const binding = match(event, ROW_TABLES, 'list', state);
    if (binding === null) return;
    switch (binding.id) {
      case 'list.up':
        move(deps, issue, -1);
        break;
      case 'list.down':
        move(deps, issue, 1);
        break;
      case 'list.home':
        land(deps, 0);
        break;
      case 'list.end':
        land(deps, deps.rows().length - 1);
        break;
      case 'list.page-prev':
        deps.page(-1);
        break;
      case 'list.page-next':
        deps.page(1);
        break;
      case 'list.open':
      case 'list.open.key':
        deps.open(issue, false);
        break;
      case 'list.open.new':
        deps.open(issue, true);
        break;
      case 'list.edit':
        deps.edit(issue);
        break;
      case 'list.star':
        deps.star(issue);
        break;
      case 'list.assign-me':
        deps.assignMe(issue);
        break;
      case 'list.sidebar':
        deps.sidebar();
        break;
      case 'list.select':
        deps.select.toggle(issue);
        break;
      case 'list.select.up':
        deps.select.extend(issue, -1);
        break;
      case 'list.select.down':
        deps.select.extend(issue, 1);
        break;
      case 'list.select.page':
        deps.select.page();
        break;
      default:
        return;
    }
    event.preventDefault();
  };
}

/** One row up or down from `issue`; the focus move scrolls the row into view (the DOM focus) and
 * announces it (§6). */
function move(deps: RowKeysDeps, issue: Issue, step: 1 | -1): void {
  const rows = deps.rows();
  const at = rows.findIndex((row) => row.id === issue.id);
  if (at === -1) return;
  land(deps, at + step);
}

/** Focuses the row at `index`, if it exists. */
function land(deps: RowKeysDeps, index: number): void {
  const row = deps.rows()[index];
  if (!row) return;
  deps.focus(row.id);
}
