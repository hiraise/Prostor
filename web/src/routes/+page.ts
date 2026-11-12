// The root address opens the person's home page (UI-004). Their preferences load first when they
// are signed in; otherwise (or when that fails) the default home leads to signing in.
import { redirect } from '@sveltejs/kit';

import { ensureSession, preferences } from '#lib/session/index.ts';
import { homeHref } from '#lib/shell/sections.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
  try {
    if (await ensureSession(fetch)) await preferences.ensure(fetch);
  } catch {
    // The app's layout shows what went wrong.
  }
  redirect(307, homeHref());
};
