// The watcher list of an issue (NTF-015, NTF-016, plan step 1.10 §4.3): who watches it, and
// replacing the whole list. The count on the issue row follows the store, as the star's does.
import { api } from '#lib/api/client.ts';
import type { Issue, UserRef } from '#lib/api/schema.gen.ts';
import { currentUser } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

/** Who watches the issue, in the order they came — behind `issue.watchers.read`. */
export async function loadWatchers(issue: number): Promise<UserRef[]> {
  const answer = await api.get('/issues/{id}/watchers', { path: { id: issue } });
  return answer.users;
}

/**
 * Replaces the watcher list. The stored issue shows the new count at once and one's own star with
 * it; a refusal rolls both back and reaches the caller as a rejected promise.
 */
export async function saveWatchers(issue: Issue, users: number[]): Promise<UserRef[]> {
  const me = currentUser()?.id ?? null;
  const changes: Partial<Issue> = {
    watchers: issue.watchers === null ? null : users.length,
    starred: me !== null && users.includes(me),
  };
  let saved: UserRef[] = [];
  await store.update('issue', issue.id, changes, async () => {
    const answer = await api.put('/issues/{id}/watchers', {
      path: { id: issue.id },
      body: { users },
    });
    saved = answer.users;
    const row = store.entities.table('issue').get(issue.id);
    if (!row) throw new Error('issue left the store');
    return { ...row, ...changes };
  });
  return saved;
}
