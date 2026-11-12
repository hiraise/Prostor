// The header filter of boards (BRD-070, BRD-081): in the address as `?q=`, and kept here while the
// person stays under `/boards`, so it follows them from board to board. Leaving boards clears it
// (the boards layout); a new tab starts without it.
import { boardHref } from '../boards.ts';

let kept = $state('');

export const headerFilter = {
  /** The filter kept for the next board; empty for none. Reactive. */
  get value(): string {
    return kept;
  },
  set(q: string): void {
    kept = q.trim() === '' ? '' : q;
  },
  clear(): void {
    kept = '';
  },
};

/** Whether an address is under `/boards`, where the filter stays. */
export function underBoards(pathname: string): boolean {
  const entry = boardHref.entry();
  return pathname === entry || pathname.startsWith(`${entry}/`);
}

/** The address of a board or one of its sprints with the filter. */
export function boardAddress(id: number, sprint: number | null, q: string): string {
  const base = sprint === null ? boardHref.board(id) : boardHref.sprint(id, sprint);
  return q === '' ? base : `${base}?q=${encodeURIComponent(q)}`;
}
