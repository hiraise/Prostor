// Opening and closing the new-issue form, the same for the dialog and the page (ISS-005, ISS-010).
import { preferences } from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import type { NewIssueLink } from '#lib/api/schema.gen.ts';

import { linkTypes, subtaskLink } from '../links/links.svelte.ts';
import * as m from '../messages.ts';
import type { DraftSession } from './drafts.svelte.ts';
import { openNewIssue, type NewIssueTarget } from './open.svelte.ts';
import { creatableProjects, defaultProject } from './projects.ts';

/** Starts the form's draft: the one asked for, or a new one in the default project. `false`
 * when the person may create issues nowhere. */
export async function beginDraft(session: DraftSession, target: NewIssueTarget): Promise<boolean> {
  if (target.draft !== undefined && (await session.open(target.draft))) return true;
  const projects = await creatableProjects();
  const project = defaultProject(
    projects,
    target.project,
    preferences.chosen('last_project') ?? null,
  );
  if (!project) return false;
  await session.start({ project: project.id, links: await startLinks(target) });
  return true;
}

/** The form closes without publishing: an empty draft goes, any other stays with a toast. */
export async function endDraft(session: DraftSession): Promise<void> {
  const id = session.id;
  const result = await session.close();
  if (result !== 'kept' || id === null) return;
  toasts.show({
    message: m.draftSaved(),
    action: {
      label: m.open(),
      run: () => {
        openNewIssue({ draft: id });
      },
    },
  });
}

/** The links a form started from an issue begins with (ISS-009). */
async function startLinks(target: NewIssueTarget): Promise<NewIssueLink[]> {
  const links: NewIssueLink[] = [];
  if (target.parent !== undefined) {
    const parent = subtaskLink(await linkTypes(), target.parent);
    if (parent) links.push(parent);
  }
  if (target.link) links.push(target.link);
  return links;
}
