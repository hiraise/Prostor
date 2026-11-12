// Asking for a password-reset link (AUTH-008); before the first-run setup there is nobody to reset.
import { redirect } from '@sveltejs/kit';

import { setupHref } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  const { bootstrap } = await parent();
  if (bootstrap?.setup_required) redirect(307, setupHref());
  return { emailEnabled: bootstrap?.email_enabled ?? false };
};
