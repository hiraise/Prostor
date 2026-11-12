// The value sets of the instance (FLD-025, FLD-029); the search lives in the address (UI-006).
import { ensureValueSets, setFilters } from '#lib/features/fields/index.ts';
import { canSeeFields } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url }) => {
  await parent();
  const filters = setFilters(url.searchParams);
  if (!canSeeFields()) return { sets: null, filters };
  return { sets: await ensureValueSets(filters), filters };
};
