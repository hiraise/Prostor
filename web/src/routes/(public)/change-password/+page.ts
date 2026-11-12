// A new password before anything else (AUTH-010): only for a session that must choose one.
import { redirect } from '@sveltejs/kit';

import { ensureSession, safeNext, signInHref } from '#lib/session/index.ts';
import { homeHref } from '#lib/shell/sections.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch, untrack }) => {
  const next = untrack(() => safeNext(url.searchParams.get('next'))) ?? homeHref();
  const session = await ensureSession(fetch);
  if (!session) redirect(307, signInHref(next));
  if (!session.password_change_required) redirect(307, next);
  return { next };
};
