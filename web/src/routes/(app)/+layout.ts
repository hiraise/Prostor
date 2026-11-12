// Every page of the app needs a signed-in person (AUTH-001) who has chosen their own password
// (AUTH-010). Before the first-run setup there is nobody to sign in: the setup wizard opens
// instead (ADM-047).
import { redirect } from '@sveltejs/kit';

import {
  changePasswordHref,
  ensureAccess,
  ensureSession,
  preferences,
  setupHref,
  signInHref,
} from '#lib/session/index.ts';

import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ parent, url, fetch, untrack }) => {
  const { bootstrap } = await parent();
  // Without the configuration the root layout shows why and keeps retrying.
  if (!bootstrap) return {};
  if (bootstrap.setup_required) redirect(307, setupHref());
  const session = await ensureSession(fetch);
  // The session is checked once per page load; leaving the address out keeps this load from
  // running again on every navigation.
  if (!session)
    redirect(
      307,
      untrack(() => signInHref(url.pathname + url.search)),
    );
  // AUTH-010: nothing else until a new password is chosen.
  if (session.password_change_required)
    redirect(
      307,
      untrack(() => changePasswordHref(url.pathname + url.search)),
    );
  // What the person may do shapes navigation and pages (ACC-062); their interface settings
  // (UI-018) load beside it, and a failure there leaves the defaults.
  await Promise.all([ensureAccess(fetch), preferences.ensure(fetch)]);
  return {};
};
