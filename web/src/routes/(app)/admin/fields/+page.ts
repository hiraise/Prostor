// The fields of the instance (FLD-025); the search lives in the address (UI-006).
import { ensureFields, fieldFilters } from '#lib/features/fields/index.ts';
import { canSeeFields } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url }) => {
  await parent();
  const filters = fieldFilters(url.searchParams);
  if (!canSeeFields()) return { fields: null, filters };
  return { fields: await ensureFields(filters), filters };
};
