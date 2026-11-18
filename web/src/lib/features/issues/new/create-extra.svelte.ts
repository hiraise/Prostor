// The issue page's entries for the shell's «Создать» menu (step 2.2 §7, ISS-047): while an issue
// is open, `c` offers its subtask and a linked issue beside «Задачу». The page sets the entries
// for as long as it is shown; the shell reads them where it builds the menu.
import type { MenuEntry } from '#lib/ui/menu.ts';

let extra: (() => MenuEntry[]) | null = $state(null);

/** The page says what its issue offers; `null` clears. */
export function setIssueCreateExtra(entries: (() => MenuEntry[]) | null): void {
  extra = entries;
}

/** The open issue's entries for «Создать», in menu order. */
export function issueCreateExtra(): MenuEntry[] {
  return extra?.() ?? [];
}
