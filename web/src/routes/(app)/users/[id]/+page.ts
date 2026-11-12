// A person's profile (UI-010, ACC-015). One's own opens the account page; the ID of a deleted
// account leads to the account that took its place (ACC-013); anything else missing or hidden is
// "not found", so the page never reveals that an account exists.
import { error, redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';

import { openProfile } from '#lib/features/admin/index.ts';
import { currentUser } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, parent }) => {
  await parent();
  const id = Number(params.id);
  if (!/^\d+$/.test(params.id) || !Number.isSafeInteger(id)) error(404, 'Not found');
  if (currentUser()?.id === id) redirect(307, resolve('/(app)/account'));
  const found = await openProfile(id);
  if (!found) error(404, 'Not found');
  if ('moved' in found) redirect(307, resolve('/(app)/users/[id]', { id: String(found.moved) }));
  return { profile: found.handle };
};
