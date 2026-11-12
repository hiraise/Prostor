// Accounts in the data store (architecture.md §15) and people search (ACC-002, ACC-070).
// The `account` kind holds the directory's short form and the profile's full form.
import { api } from '#lib/api/client.ts';
import type { Account, AccountSummary, UserRef } from '#lib/api/schema.gen.ts';
import { store, type EntityHandle, type Page } from '#lib/store/index.ts';

import * as m from './messages.ts';

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    account: AccountSummary | Account;
  }
}

store.defineKind('account', {
  // There is no batch endpoint: a refetch loads each account; they are few at a time.
  async load(ids) {
    const found = await Promise.all(
      ids.map((id) =>
        api
          .get('/users/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    return found.filter((account): account is Account => account !== null);
  },
});

/** A profile in full form (ACC-015). */
export function ensureAccount(id: number): Promise<EntityHandle<AccountSummary | Account>> {
  return store.ensureEntity('account', id, { form: 'full' });
}

/** Puts an account the server just returned into the store. */
export function keepAccount(account: Account | AccountSummary, form: 'short' | 'full'): void {
  store.entities.table('account').upsert(account, form);
}

/** Which accounts a search includes. */
export type AccountState = 'all' | 'enabled' | 'disabled';

/** One page of people by name. */
export function findPeople(
  text: string,
  options: {
    state?: AccountState;
    cursor?: string | null;
    limit?: number;
    signal?: AbortSignal;
  } = {},
): Promise<Page<AccountSummary>> {
  return api.get('/users', {
    query: {
      q: text.trim() || undefined,
      state: options.state,
      cursor: options.cursor ?? undefined,
      limit: options.limit ?? 20,
    },
    signal: options.signal,
  });
}

/** Whatever names a person: a reference, a directory row or a profile. */
export type PersonLike = Pick<UserRef, 'id' | 'full_name' | 'username' | 'avatar_color'> &
  Partial<Pick<UserRef, 'kind' | 'pseudonym' | 'avatar_url'>>;

/** The name to show for a person: pseudonyms and deleted accounts in the viewer's language. */
export function personName(person: PersonLike): string {
  if (person.kind === 'deleted') return m.deletedPerson();
  if (person.kind === 'masked' && person.pseudonym != null)
    return m.maskedPerson({ number: person.pseudonym });
  return person.full_name;
}

/** The username to show next to the name, if the viewer may see it. */
export function personUsername(person: PersonLike): string | null {
  return person.kind === 'masked' || person.kind === 'deleted' ? null : person.username;
}
