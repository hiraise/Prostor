// The first-run setup wizard (ADM-047): only while nobody has run it.
import { redirect } from '@sveltejs/kit';

import { signInHref } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  const { bootstrap } = await parent();
  if (bootstrap && !bootstrap.setup_required) redirect(307, signInHref());
  return {};
};
