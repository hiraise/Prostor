// The user directory (ACC-002) loads with the page, so a preload fills the cache. Without the
// right to see it, or when it fails, the page shows why itself.
import { ensureDirectory } from '#lib/features/admin/index.ts';
import { canSeeDirectory } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  await parent();
  const directory = canSeeDirectory() ? await ensureDirectory().catch(() => null) : null;
  return { directory };
};
