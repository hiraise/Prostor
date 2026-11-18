// Changes of an issue from its page (ISS-018, ISS-020, ISS-028, ISS-030, ISS-032, ISS-034,
// ISS-035, ISS-041): each shows at once in the stored row and rolls back when the server refuses.
import { ApiError, api } from '#lib/api/client.ts';
import { currentUser } from '#lib/session/index.ts';
import { announce } from '#lib/ui/announce.svelte.ts';
import { showRuleMessages } from '#lib/ui/rule-messages.ts';
import { showError } from '#lib/ui/toast.svelte.ts';
import type {
  Audience,
  AudienceCandidates,
  Issue,
  IssuePatch,
  MovePreview,
  MoveResult,
  Tag,
  TagRef,
  UserRef,
} from '#lib/api/schema.gen.ts';
import { store, type Changes } from '#lib/store/index.ts';

import { withChoices } from './fields/field-values.ts';
import { saveField } from './fields/save.ts';
import * as m from './messages.ts';

/** Edits that answer `204`: the saved row is the stored one with the edit applied. */
async function quietly(id: number, changes: Changes<Issue>, send: () => Promise<unknown>) {
  await store.update('issue', id, changes, async () => {
    await send();
    const row = store.entities.table('issue').get(id);
    if (!row) throw new Error('issue left the store');
    return { ...row, ...changes };
  });
}

export function setStar(issue: Issue, on: boolean): Promise<void> {
  const watchers = issue.watchers === null ? null : issue.watchers + (on ? 1 : -1);
  const path = { path: { id: issue.id } };
  return quietly(issue.id, { starred: on, watchers }, () =>
    on ? api.put('/issues/{id}/star', path) : api.delete('/issues/{id}/star', path),
  );
}

export function setVote(issue: Issue, on: boolean): Promise<void> {
  const path = { path: { id: issue.id } };
  return quietly(issue.id, { voted: on, votes: issue.votes + (on ? 1 : -1) }, () =>
    on ? api.put('/issues/{id}/vote', path) : api.delete('/issues/{id}/vote', path),
  );
}

/** Assigns the issue to the signed-in person (step 2.2 §4, the `m` of the list and the page):
 * the same field save the editors use, no API of its own. An issue without an editable single
 * assignee field does nothing. */
export function assignToMe(row: Issue): void {
  const me = currentUser();
  const entry = row.fields.find((field) => field.field.builtin === 'assignee');
  if (me === null || entry === undefined || !entry.editable || entry.field.multi) return;
  const person: UserRef = {
    id: me.id,
    username: me.username,
    full_name: me.full_name,
    avatar_color: me.avatar_color,
    avatar_url: me.avatar_url,
    disabled: false,
    kind: 'person',
    pseudonym: null,
  };
  saveField(row.id, withChoices(entry, [{ kind: 'user', user: person }]), {
    kind: 'set',
    value: me.id,
  }).then(
    () => {
      announce(m.assignedToMe());
    },
    (error: unknown) => {
      showError(error);
    },
  );
}

/** Puts a tag on the issue or takes it off (ISS-034). */
export async function setTag(issue: Issue, tag: TagRef, on: boolean): Promise<void> {
  const others = issue.tags.filter((other) => other.id !== tag.id);
  const tags = on ? [...others, tag] : others;
  const path = { path: { id: issue.id, tag_id: tag.id } };
  await store.update('issue', issue.id, { tags }, () =>
    on
      ? api.put('/issues/{id}/tags/{tag_id}', path)
      : api.delete('/issues/{id}/tags/{tag_id}', path),
  );
}

/** Tags the viewer may put on issues, by name (ISS-035). */
export async function findTags(text: string, signal?: AbortSignal): Promise<Tag[]> {
  const page = await api.get('/tags', {
    query: { q: text.trim() || undefined, scope: 'all', limit: 50 },
    signal,
  });
  return page.items.filter((tag) => tag.abilities.apply);
}

export function createTag(name: string): Promise<Tag> {
  return api.post('/tags', { body: { name: name.trim() } });
}

/** The audience as the server takes it. */
export function audienceInput(audience: Audience): { users: number[]; groups: number[] } {
  return {
    users: audience.users.map((user) => user.id),
    groups: audience.groups.map((group) => group.id),
  };
}

export function audienceCandidates(
  project: number,
  text: string,
  signal?: AbortSignal,
): Promise<AudienceCandidates> {
  return api.get('/projects/{id}/audience-candidates', {
    path: { id: project },
    query: { q: text.trim() || undefined },
    signal,
  });
}

/** The summary and description, as the conflict view compares them (ISS-051). */
export interface Texts {
  summary: string;
  description: string;
}

/** The texts edit mode saves in one request (ISS-018). */
export interface TextEdit {
  summary?: string;
  description?: string;
  audience?: Audience;
}

/**
 * Saves the summary, description and audience on the `text_version` editing began with. A newer
 * text on the server ends as the store's `ConflictError` (ISS-051): the version travels with the
 * request, so a retry by the store is refused again and nothing is overwritten silently.
 */
export async function saveTexts(id: number, edit: TextEdit, textVersion: number): Promise<Issue> {
  const body: IssuePatch = {};
  if (edit.summary !== undefined) body.summary = edit.summary;
  if (edit.description !== undefined) body.description = edit.description;
  if (edit.audience !== undefined) body.audience = audienceInput(edit.audience);
  if (body.summary !== undefined || body.description !== undefined) body.text_version = textVersion;
  return store.update('issue', id, edit, async () => {
    const answer = await api.patch('/issues/{id}', { path: { id }, body });
    showRuleMessages(answer.messages);
    return answer.issue;
  });
}

export function previewMove(issue: number, project: number): Promise<MovePreview> {
  return api.get('/issues/{id}/move-preview', { path: { id: issue }, query: { project } });
}

/** Moves the issue; the answer has no issue when the viewer cannot see it there (then the
 * caller leaves the page and `forgetIssue`s it). */
export async function moveIssue(issue: number, project: number): Promise<MoveResult> {
  const result = await api.post('/issues/{id}/move', { path: { id: issue }, body: { project } });
  showRuleMessages(result.messages);
  if (result.issue) store.entities.table('issue').upsert(result.issue, 'full');
  return result;
}

/** Drops a deleted or no longer visible issue from the store, once its page is left. */
export function forgetIssue(id: number): void {
  store.entities.table('issue').remove(id);
}

export async function deleteIssue(id: number): Promise<void> {
  try {
    await api.delete('/issues/{id}', { path: { id } });
  } catch (error) {
    // Someone deleted it first: the result is the same.
    if (!(error instanceof ApiError && error.code === 'not_found')) throw error;
  }
}
