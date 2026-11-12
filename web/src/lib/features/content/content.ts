// Calls of the Markdown editor: the preview (CNT-029) and the people a text may mention (CNT-054).
// Both are answers for the moment, not entities, so they bypass the store.
import { api } from '#lib/api/client.ts';
import type { MarkdownProfile, Rendered, UserRef } from '#lib/api/schema.gen.ts';

/** How many people the mention list offers. */
export const MENTION_LIMIT = 8;

/** The text rendered as the server will show it, with the caller's staged `uploads` (CNT-034). */
export function previewMarkdown(
  text: string,
  profile: MarkdownProfile,
  issue: number | undefined,
  uploads: readonly number[],
  signal?: AbortSignal,
): Promise<Rendered> {
  const body = { text, profile, issue: issue ?? null };
  return api.post('/markdown', {
    body: uploads.length > 0 ? { ...body, uploads: [...uploads] } : body,
    signal,
  });
}

/** People who can see `issue` whose name or username starts with `query`. */
export async function mentionCandidates(
  issue: number,
  query: string,
  signal?: AbortSignal,
): Promise<UserRef[]> {
  const found = await api.get('/issues/{id}/mention-candidates', {
    path: { id: issue },
    query: { q: query, limit: MENTION_LIMIT },
    signal,
  });
  return found.users;
}

/** An `@word` being typed right before the caret: where it starts and the word so far. */
export function mentionAt(text: string, caret: number): { from: number; query: string } | null {
  const before = text.slice(text.lastIndexOf('\n', caret - 1) + 1, caret);
  // After a space, a bracket or the start, like the server's mention rule; not inside an email.
  const match = /(?:^|[\s([{"'])@([\p{L}\p{N}._-]{0,50})$/u.exec(before);
  if (!match) return null;
  const query = match[1] ?? '';
  return { from: caret - query.length - 1, query };
}
