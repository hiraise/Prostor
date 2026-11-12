// Cloning an issue into a copy or the actor's draft (AUT-066, §3.13). The project's `clone_issue`
// toggle decides whether the menus offer it at all: it is read from the rules endpoint lazily,
// once per project, when a menu is about to need it — reading the endpoint needs no settings
// right, only to see the project.
import { goto } from '$app/navigation';
import { SvelteSet } from 'svelte/reactivity';

import { api } from '#lib/api/client.ts';
import type { Issue } from '#lib/api/schema.gen.ts';
import { store } from '#lib/store/index.ts';
import { showRuleMessages } from '#lib/ui/rule-messages.ts';
import { showError } from '#lib/ui/toast.svelte.ts';

import { issueHref } from './issues.ts';
import * as m from './messages.ts';

/** The projects whose toggle is known (reactive, so menus appear once it lands). */
let known = $state.raw<Record<number, boolean>>({});
/** The look-ups in flight; a failed one leaves, so a later intent asks again. */
const asking = new SvelteSet<number>();

/** Asks for the project's `clone_issue` toggle — once per project, at most one request at a time. */
export function askCloning(project: number): void {
  if (known[project] !== undefined || asking.has(project)) return;
  asking.add(project);
  api
    .get('/projects/{id}/rules', { path: { id: project } })
    .then(
      (answer) => {
        known = {
          ...known,
          [project]: answer.items.some((rule) => rule.rule === 'clone_issue' && rule.enabled),
        };
      },
      () => undefined,
    )
    .finally(() => asking.delete(project));
}

/** Whether the project lets the viewer clone; false until its toggle is known. */
export function cloningKnown(project: number): boolean {
  return known[project] === true;
}

/** Tests start with nothing known, nothing in flight and nothing busy. */
export function resetCloning(): void {
  known = {};
  asking.clear();
  busy.clear();
}

/** The issues being cloned right now; the menus wait instead of asking twice. */
const busy = new SvelteSet<number>();

export function cloning(id: number): boolean {
  return busy.has(id);
}

function mark(id: number, on: boolean): void {
  if (on) busy.add(id);
  else busy.delete(id);
}

/** Clones the issue as a published copy (§3.10): the copy joins the store, the toast opens it. */
export async function cloneIssue(issue: Issue): Promise<void> {
  if (busy.has(issue.id)) return;
  mark(issue.id, true);
  try {
    const answer = await api.post('/issues/{id}/clone', { path: { id: issue.id } });
    store.entities.table('issue').upsert(answer.issue, 'full');
    showRuleMessages(answer.messages, (message) =>
      message.code === 'clone_created'
        ? {
            label: m.open(),
            // issueHref resolves the address.
            // eslint-disable-next-line svelte/no-navigation-without-resolve
            run: () => goto(issueHref(message.params.issue ?? answer.issue)),
          }
        : undefined,
    );
  } catch (error) {
    showError(error);
  } finally {
    mark(issue.id, false);
  }
}

/** Clones the issues as the actor's drafts (§3.10): one toast per source, no links to offer. */
export async function cloneAsDrafts(issues: readonly Issue[]): Promise<void> {
  const waiting = issues.filter((issue) => !busy.has(issue.id));
  if (waiting.length === 0) return;
  for (const issue of waiting) mark(issue.id, true);
  try {
    const answer = await api.post('/issues/clone-drafts', {
      body: { issues: waiting.map((issue) => issue.id) },
    });
    for (const draft of answer.drafts) store.entities.table('issue').upsert(draft, 'full');
    showRuleMessages(answer.messages);
  } catch (error) {
    showError(error);
  } finally {
    for (const issue of waiting) mark(issue.id, false);
  }
}
