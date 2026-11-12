// One value set (FLD-029…FLD-049); a missing one is "not found".
import { ApiError } from '#lib/api/client.ts';
import { ensureValueSet } from '#lib/features/fields/index.ts';
import { canSeeFields } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, params }) => {
  await parent();
  if (!canSeeFields() || !/^\d{1,15}$/.test(params.id)) return { set: null };
  try {
    return { set: await ensureValueSet(Number(params.id)) };
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_found') return { set: null };
    throw error;
  }
};
