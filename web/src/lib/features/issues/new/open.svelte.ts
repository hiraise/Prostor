// Opening the new-issue form (ISS-005, UI-002): a dialog on wide screens, its own page on phones.
// Every entry point calls `openNewIssue`; the dialog lives once in the shell (`NewIssueHost`).
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';

import type { NewIssueLink } from '#lib/api/schema.gen.ts';
import { NARROW } from '#lib/theme/breakpoints.ts';

import { linkParam } from '../links/links.svelte.ts';

export interface NewIssueTarget {
  /** The key of the project the entry point stands in. */
  project?: string;
  /** A draft to continue. */
  draft?: number;
  /** The new issue is a subtask of this issue (ISS-009). */
  parent?: number;
  /** The new issue is linked so (ISS-009, LNK-006). */
  link?: NewIssueLink;
}

class NewIssueDialogState {
  open = $state(false);
  target = $state<NewIssueTarget>({});
}

export const newIssueDialog = new NewIssueDialogState();

/** The project of the list on screen (ISS-005): new issues started anywhere take it. */
let standing: string | undefined;

/** Makes new issues start in this project until the returned function is called. */
export function standInProject(key: string): () => void {
  standing = key;
  return () => {
    if (standing === key) standing = undefined;
  };
}

/** The address of the new-issue page. */
export function newIssueHref(target: NewIssueTarget = {}): string {
  const base = resolve('/(app)/issues/new');
  if (target.draft !== undefined) return `${base}?draft=${String(target.draft)}`;
  const params: string[] = [];
  if (target.project) params.push(`project=${encodeURIComponent(target.project)}`);
  if (target.parent !== undefined) params.push(`parent=${String(target.parent)}`);
  if (target.link) params.push(`link=${encodeURIComponent(linkParam(target.link))}`);
  return params.length > 0 ? `${base}?${params.join('&')}` : base;
}

function isNarrow(): boolean {
  return typeof window !== 'undefined' && window.matchMedia(NARROW).matches;
}

export function openNewIssue(given: NewIssueTarget = {}): void {
  const target =
    given.project === undefined && given.draft === undefined && standing !== undefined
      ? { ...given, project: standing }
      : given;
  if (isNarrow()) {
    // newIssueHref resolves the address.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(newIssueHref(target));
    return;
  }
  newIssueDialog.target = target;
  newIssueDialog.open = true;
}
