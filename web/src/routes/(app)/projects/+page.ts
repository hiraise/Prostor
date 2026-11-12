// The projects list (PRJ-010, PRJ-013); its search lives in the address (UI-006). People who may
// delete projects also get those being deleted (PRJ-065); archived ones load when their section
// opens.
import { canDeleteSome, ensureProjects } from '#lib/features/projects/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url }) => {
  await parent();
  const query = url.searchParams.get('q')?.trim() ?? '';
  const [active, deleting] = await Promise.all([
    ensureProjects('active', query),
    canDeleteSome() ? ensureProjects('deleting', query).catch(() => null) : null,
  ]);
  return { active, deleting, query };
};
