// Personal API tokens in the data store (architecture.md §15; AUTH-029, AUTH-030).
import { api } from '#lib/api/client.ts';
import type { ApiToken, CreatedApiToken, TokenScope } from '#lib/api/schema.gen.ts';
import { store, type ListHandle } from '#lib/store/index.ts';

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    api_token: ApiToken;
  }
}

/** Tokens per page; people rarely have more. */
const PAGE = 50;

store.defineKind('api_token', {
  // There is no endpoint for one token: read the first page and keep the asked ones.
  async load(ids) {
    const page = await api.get('/users/me/tokens', { query: { limit: 100 } });
    return page.items.filter((token) => ids.includes(token.id));
  },
});

/** The caller's tokens, newest first. */
export function ensureTokens(): Promise<ListHandle<ApiToken>> {
  return store.ensureList('api_token', {
    key: ['mine'],
    fetch: (cursor, signal) =>
      api.get('/users/me/tokens', { query: { limit: PAGE, cursor: cursor ?? undefined }, signal }),
  });
}

export function createToken(name: string, scopes: TokenScope[]): Promise<CreatedApiToken> {
  return api.post('/users/me/tokens', { body: { name, scopes } });
}

export async function deleteToken(id: number): Promise<void> {
  await api.delete('/users/me/tokens/{id}', { path: { id } });
  store.entities.table('api_token').remove(id);
}

/** Access to one area: none, reading, or reading and changing. */
export type Access = 'none' | 'read' | 'write';

/** The scopes for a chosen access to the tracker and to administration. */
export function scopesFor(tracker: Access, admin: Access): TokenScope[] {
  const scopes: TokenScope[] = [];
  if (tracker !== 'none') scopes.push(tracker === 'write' ? 'tracker.write' : 'tracker.read');
  if (admin !== 'none') scopes.push(admin === 'write' ? 'admin.write' : 'admin.read');
  return scopes;
}

/** A token's access to each area, from its scopes. */
export function accessOf(scopes: readonly TokenScope[]): { tracker: Access; admin: Access } {
  const level = (read: TokenScope, write: TokenScope): Access =>
    scopes.includes(write) ? 'write' : scopes.includes(read) ? 'read' : 'none';
  return {
    tracker: level('tracker.read', 'tracker.write'),
    admin: level('admin.read', 'admin.write'),
  };
}
