// One role (ACC-054); a missing or hidden role is "not found" (architecture §8).
import { ApiError } from '#lib/api/client.ts';
import { ensureRole, ensureRoleAssignments, permissionCatalog } from '#lib/features/admin/index.ts';
import { canSeeRoles } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, params }) => {
  await parent();
  const id = Number(params.id);
  if (!canSeeRoles() || !Number.isSafeInteger(id) || id <= 0) {
    return { role: null, holders: null, catalog: [] };
  }
  try {
    const [role, holders, catalog] = await Promise.all([
      ensureRole(id),
      ensureRoleAssignments(id),
      permissionCatalog(),
    ]);
    return { role, holders, catalog };
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_found') {
      return { role: null, holders: null, catalog: [] };
    }
    throw error;
  }
};
