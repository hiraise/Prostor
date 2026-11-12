// The mail settings of the instance (NTF-004): the «Почта» screen of the administration area.
import { canSeeMail } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  await parent();
  return { maySee: canSeeMail() };
};
