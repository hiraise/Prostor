// One group (ACC-021…ACC-032, ACC-060). The address of a deleted group leads to its replacement
// (ACC-013); a project team's leads to its project's people page, where the team changes (ACC-033);
// a missing or hidden one shows "not found".
import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';

import { openGroup } from '#lib/features/admin/index.ts';
import { canSeeGroups } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, params }) => {
  await parent();
  if (!canSeeGroups()) return { allowed: false, group: null };
  if (!/^\d{1,15}$/.test(params.id)) return { allowed: true, group: null };
  const opened = await openGroup(Number(params.id));
  if (opened.kind === 'replaced') {
    redirect(307, resolve('/(app)/admin/groups/[id]', { id: String(opened.id) }));
  }
  if (opened.kind === 'found') {
    const project = opened.group.current?.team_of;
    if (project != null) {
      redirect(307, resolve('/(app)/projects/[key]/settings/people', { key: String(project) }));
    }
  }
  return { allowed: true, group: opened.kind === 'found' ? opened.group : null };
};
