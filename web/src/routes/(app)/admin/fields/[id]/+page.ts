// One field of the instance (FLD-016…FLD-025); a missing or hidden one is "not found".
import { ApiError } from '#lib/api/client.ts';
import { ensureField } from '#lib/features/fields/index.ts';
import { canSeeFields } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, params }) => {
  await parent();
  if (!canSeeFields() || !/^\d{1,15}$/.test(params.id)) return { field: null };
  try {
    return { field: await ensureField(Number(params.id)) };
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_found') return { field: null };
    throw error;
  }
};
