// Opening the new-board form (BRD-003, UI-002): a dialog on wide screens, its own page on phones.
// Every entry point calls `openNewBoard`; the dialog lives once in the shell (`NewBoardHost`).
import { goto } from '$app/navigation';

import type { ProjectChoice } from '#lib/features/projects/index.ts';
import { NARROW } from '#lib/theme/breakpoints.ts';

import { boardHref } from '../boards.ts';

class NewBoardDialogState {
  open = $state(false);
  /** The project the form starts with. */
  project = $state<ProjectChoice | undefined>(undefined);
}

export const newBoardDialog = new NewBoardDialogState();

/** The address of the new-board page; `project` is a project ID to start with. */
export function newBoardHref(project?: number): string {
  const base = boardHref.create();
  return project === undefined ? base : `${base}?project=${String(project)}`;
}

export function openNewBoard(project?: ProjectChoice): void {
  if (typeof window !== 'undefined' && window.matchMedia(NARROW).matches) {
    // newBoardHref resolves the address.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(newBoardHref(project?.id));
    return;
  }
  newBoardDialog.project = project;
  newBoardDialog.open = true;
}

/**
 * After a board is created it opens with its settings panel (BRD-003, BRD-086); the board page
 * reads `?settings`.
 */
export function openCreated(id: number): Promise<void> {
  // boardHref resolves the address.
  // eslint-disable-next-line svelte/no-navigation-without-resolve
  return goto(`${boardHref.board(id)}?settings`);
}
