// The roles list (ACC-053); its search lives in the address (UI-006).
import { ensureRoles, permissionCatalog, roleFilters } from '#lib/features/admin/index.ts';
import { canSeeRoles } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url }) => {
  await parent();
  const filters = roleFilters(url.searchParams);
  if (!canSeeRoles()) return { roles: null, catalog: [], filters };
  const [roles, catalog] = await Promise.all([ensureRoles(filters), permissionCatalog()]);
  return { roles, catalog, filters };
};
