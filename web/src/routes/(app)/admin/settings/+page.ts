// The general settings of the instance (ADM-002): the «Настройки» screen of the administration
// area. The mail settings have their own screen.
import { canSeeSettings } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  await parent();
  return { maySee: canSeeSettings() };
};
