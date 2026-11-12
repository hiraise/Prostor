// Calls for comments (CNT-001, CNT-003…CNT-005, CNT-007, CNT-018). Every answer goes into the
// store, so the stream's cards follow it.
import { api } from '#lib/api/client.ts';
import { showRuleMessages } from '#lib/ui/rule-messages.ts';
import type { Audience, Comment, CurrentUser, Issue, UserRef } from '#lib/api/schema.gen.ts';
import { store } from '#lib/store/index.ts';

import { audienceInput } from '../actions.ts';
import { isRestricted } from '../audience/audience.ts';
import { issueHref } from '../issues.ts';

export interface NewCommentInput {
  text: string;
  uploads: number[];
  /** `null`: the same people as the issue. */
  audience: Audience | null;
}

export async function postComment(issue: number, input: NewCommentInput): Promise<Comment> {
  const answer = await api.post('/issues/{id}/comments', {
    path: { id: issue },
    body: {
      text: input.text,
      ...(input.uploads.length > 0 ? { uploads: input.uploads } : {}),
      ...(input.audience && isRestricted(input.audience)
        ? { audience: audienceInput(input.audience) }
        : {}),
    },
  });
  showRuleMessages(answer.messages);
  store.put('comment', [answer.comment], 'full');
  return answer.comment;
}

/** Saves a new text and audience on the version the row holds; a `conflict` throws `ConflictError`. */
export function updateComment(
  id: number,
  text: string,
  audience: Audience,
  uploads: number[] = [],
): Promise<Comment> {
  return store.update('comment', id, { text, audience }, ({ version }) =>
    api.patch('/comments/{id}', {
      path: { id },
      body: {
        text,
        audience: audienceInput(audience),
        ...(uploads.length > 0 ? { add_uploads: uploads } : {}),
        version,
      },
    }),
  );
}

/** Adds staged uploads to a comment (CNT-018). */
export function addCommentFiles(id: number, uploads: number[]): Promise<Comment> {
  return store.update('comment', id, {}, ({ version }) =>
    api.patch('/comments/{id}', { path: { id }, body: { add_uploads: uploads, version } }),
  );
}

/** Deletes a comment; its placeholder takes its place (CNT-005). */
export async function deleteComment(id: number): Promise<Comment> {
  await api.delete('/comments/{id}', { path: { id } });
  const placeholder = await api.get('/comments/{id}', { path: { id } });
  store.put('comment', [placeholder], 'full');
  return placeholder;
}

export async function restoreComment(id: number): Promise<Comment> {
  const restored = await api.post('/comments/{id}/restore', { path: { id } });
  store.put('comment', [restored], 'full');
  return restored;
}

/** The address of a comment (CNT-007): the issue page scrolls to it. */
export function commentHref(issue: Pick<Issue, 'id' | 'readable_id'>, comment: number): string {
  return `${issueHref(issue)}?comment=${String(comment)}`;
}

/** The signed-in person as the author of a new comment. */
export function asAuthor(user: CurrentUser): UserRef {
  return {
    id: user.id,
    username: user.username,
    full_name: user.full_name,
    avatar_color: user.avatar_color,
    avatar_url: user.avatar_url,
    disabled: false,
    kind: 'person',
    pseudonym: null,
  };
}
