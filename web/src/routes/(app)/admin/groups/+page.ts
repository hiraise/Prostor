// The groups list (ACC-022); its search and the kind of groups shown live in the address (UI-006).
import { ensureGroups, groupKind } from '#lib/features/admin/index.ts';
import { canSeeGroups } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url }) => {
  await parent();
  const query = url.searchParams.get('q')?.trim() ?? '';
  const kind = groupKind(url.searchParams.get('kind'));
  if (!canSeeGroups()) return { groups: null, query, kind };
  return { groups: await ensureGroups(query, kind), query, kind };
};
