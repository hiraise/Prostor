// The link types of the instance (LNK-018).
import { ensureLinkTypes } from '#lib/features/links/index.ts';
import { canSeeLinkTypes } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  await parent();
  if (!canSeeLinkTypes()) return { types: null };
  return { types: await ensureLinkTypes() };
};
