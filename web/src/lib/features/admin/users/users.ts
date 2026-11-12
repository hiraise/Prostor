// Accounts as administrators manage them (ACC-002…ACC-006, ACC-009, ACC-026, ACC-059, AUTH-010,
// AUTH-014): the directory list in the data store (architecture.md §15) and the requests of the
// directory and profile pages. Rows live in the `account` kind of the access feature.
import { ApiError, api } from '#lib/api/client.ts';
import type {
  Account,
  AccountSummary,
  DisabledLabel,
  EffectiveRole,
  NewAccount,
  UserDeletionImpact,
} from '#lib/api/schema.gen.ts';
import {
  ensureAccount,
  findPeople,
  keepAccount,
  type AccountState,
} from '#lib/features/access/index.ts';
import { store, type EntityHandle, type ListHandle } from '#lib/store/index.ts';

/** A row of the `account` kind: the directory's short form or a profile's full form. */
export type DirectoryAccount = AccountSummary | Account;

/** Accounts per page of the directory. */
const PAGE = 50;

/** What the directory shows: accounts whose name, username or email starts with `text`. */
export interface DirectoryFilter {
  text: string;
  state: AccountState;
}

export const EVERYONE: DirectoryFilter = { text: '', state: 'all' };

/** The directory for a filter, by name (ACC-002). */
export function ensureDirectory(
  filter: DirectoryFilter = EVERYONE,
): Promise<ListHandle<DirectoryAccount>> {
  const text = filter.text.trim();
  return store.ensureList('account', {
    key: ['directory', { text, state: filter.state }],
    fetch: (cursor, signal) =>
      findPeople(text, { state: filter.state, cursor, limit: PAGE, signal }),
  });
}

/** A profile, an account that took the place of a deleted one, or nothing (ACC-013, ACC-015). */
export type ProfileLookup = { handle: EntityHandle<DirectoryAccount> } | { moved: number } | null;

/** The profile of account `id` in full form. */
export async function openProfile(id: number): Promise<ProfileLookup> {
  try {
    return { handle: await ensureAccount(id) };
  } catch (error) {
    if (!(error instanceof ApiError && error.code === 'not_found')) throw error;
    // The ID of a deleted account answers with the account that took its place.
    const answer = await api.get('/users/{id}', { path: { id }, accept: [404] });
    if ('id' in answer && answer.id !== id) {
      keepAccount(answer, 'full');
      return { moved: answer.id };
    }
    return null;
  }
}

/** Loads accounts again after a change made here (one profile; lists refresh instead). */
export function reloadAccount(id: number): Promise<void> {
  return store.refetch('account', [id]);
}

/** Drops deleted accounts from the store, so no view shows them again. */
export function forgetAccounts(ids: readonly number[]): void {
  const table = store.entities.table('account');
  for (const id of ids) table.remove(id);
}

export async function createAccount(account: NewAccount): Promise<Account> {
  const created = await api.post('/users', { body: account });
  keepAccount(created, 'full');
  return created;
}

/** Invites people by email (ACC-004); answers with the accounts created for them. */
export async function inviteByEmail(emails: string): Promise<AccountSummary[]> {
  const { items } = await api.post('/users/invitations', { body: { emails } });
  for (const account of items) keepAccount(account, 'short');
  return items;
}

export async function resendInvitation(id: number): Promise<void> {
  await api.post('/users/{id}/invitation', { path: { id } });
}

export async function disableAccounts(
  users: readonly number[],
  label: DisabledLabel,
  reason: string,
): Promise<void> {
  await api.post('/users/disable', { body: { users: [...users], label, reason } });
}

export async function enableAccounts(users: readonly number[]): Promise<void> {
  await api.post('/users/enable', { body: { users: [...users] } });
}

export function deletionImpact(users: readonly number[]): Promise<UserDeletionImpact> {
  return api.post('/users/deletion-impact', { body: { users: [...users] } });
}

export async function deleteAccounts(users: readonly number[], replacement: number): Promise<void> {
  await api.post('/users/delete', { body: { users: [...users], replacement } });
}

/** Makes the person choose a new password at the next sign-in (AUTH-010). */
export async function askForNewPassword(id: number): Promise<void> {
  await api.post('/users/{id}/password-change', { path: { id } });
}

/** Sends a link that confirms the account's address (AUTH-014). */
export async function sendVerification(id: number): Promise<void> {
  await api.post('/users/{id}/email-verification', { path: { id } });
}

/** The fields of a profile an administrator edits (ACC-009). */
export interface ProfileChanges {
  full_name?: string;
  username?: string;
  email?: string;
}

/**
 * Saves profile fields with the version they were edited on; the row shows them at once. A
 * conflicting change by someone else throws `ConflictError` (architecture.md §15).
 */
export function saveProfile(id: number, changes: ProfileChanges): Promise<DirectoryAccount> {
  return store.update(
    'account',
    id,
    changes,
    ({ changes: sent, version }) =>
      api.patch('/users/{id}', {
        path: { id },
        body: {
          full_name: sent.full_name ?? null,
          username: sent.username ?? null,
          email: sent.email ?? null,
          version,
        },
      }),
    { form: 'full' },
  );
}

/** Every role the account holds, with where each comes from (ACC-061). */
export async function rolesOf(id: number): Promise<EffectiveRole[]> {
  return (await api.get('/users/{id}/roles', { path: { id } })).items;
}

/** Where a role is assigned, as a dialog asks: everywhere, or in chosen projects. */
export type ScopeKind = 'global' | 'projects';

/** Where a role is assigned: everywhere, or in these projects. */
export type RoleScope = { global: true } | { projects: number[] };

/** Gives a role to an account or a group globally or in projects (ACC-059, ACC-060). */
export async function assignRole(
  holder: { user_id: number } | { group_id: number },
  roleId: number,
  scope: RoleScope,
): Promise<void> {
  await api.post('/role-assignments', {
    body: {
      role_id: roleId,
      ...holder,
      global: 'global' in scope,
      projects: 'projects' in scope ? scope.projects : [],
    },
  });
}

export async function revokeAssignment(id: number): Promise<void> {
  await api.delete('/role-assignments/{id}', { path: { id } });
}

/** Adds accounts to a group (ACC-026); answers with those that were not members yet. */
export async function addToGroup(group: number, users: readonly number[]): Promise<number[]> {
  return (
    await api.post('/groups/{id}/members', { path: { id: group }, body: { users: [...users] } })
  ).users;
}

export async function removeFromGroup(group: number, users: readonly number[]): Promise<number[]> {
  return (
    await api.post('/groups/{id}/members/remove', {
      path: { id: group },
      body: { users: [...users] },
    })
  ).users;
}

/** The part of an email before `@` reduced to what usernames allow, like the server does. */
export function usernameFromEmail(email: string): string {
  const local = email.trim().split('@')[0] ?? '';
  return (local.split('+')[0] ?? '')
    .replace(/[^A-Za-z0-9._-]/g, '')
    .replace(/^[^A-Za-z0-9]+/, '')
    .toLowerCase()
    .slice(0, 44);
}

/**
 * The addresses named in the server's field problems of an invitation: each message starts with
 * the address it is about (`anna@: …`).
 */
export function addressesIn(messages: readonly string[]): string[] {
  return messages
    .map((message) => {
      const at = message.indexOf(': ');
      return at > 0 ? message.slice(0, at) : '';
    })
    .filter((address) => address !== '');
}
