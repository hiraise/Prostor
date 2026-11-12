// The capabilities of the instance (ACC-064, ADM-023): the «Возможности» screen of the
// administration area.
import { canSeeCapabilities } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  await parent();
  return { maySee: canSeeCapabilities() };
};
