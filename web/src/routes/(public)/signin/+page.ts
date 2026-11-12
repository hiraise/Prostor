// The sign-in page (AUTH-001): skipped when already signed in, and before the first-run setup.
import { redirect } from '@sveltejs/kit';

import { endReason, ensureSession, safeNext, setupHref } from '#lib/session/index.ts';
import { homeHref } from '#lib/shell/sections.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url, fetch, untrack }) => {
  const { bootstrap } = await parent();
  if (bootstrap?.setup_required) redirect(307, setupHref());
  const next = untrack(() => safeNext(url.searchParams.get('next'))) ?? homeHref();
  if (bootstrap && (await ensureSession(fetch))) redirect(307, next);
  return {
    next,
    reason: untrack(() => endReason(url.searchParams.get('reason'))),
    emailEnabled: bootstrap?.email_enabled ?? false,
  };
};
