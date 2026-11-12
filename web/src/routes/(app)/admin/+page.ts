// The administration area opens on the first page the person may use (ADM-001).
import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';

import { canSeeDirectory, canSeeGroups, canSeeLinkTypes, canSeeRoles } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  await parent();
  if (canSeeDirectory()) redirect(307, resolve('/(app)/admin/users'));
  if (canSeeGroups()) redirect(307, resolve('/(app)/admin/groups'));
  if (canSeeRoles()) redirect(307, resolve('/(app)/admin/roles'));
  if (canSeeLinkTypes()) redirect(307, resolve('/(app)/admin/link-types'));
  return {};
};
