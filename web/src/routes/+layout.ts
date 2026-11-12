// The app is a client-rendered SPA served by the Rust server (decisions.md D-005).
import type { Bootstrap } from '#lib/api/schema.gen.ts';
import { api } from '#lib/api/client.ts';
import { BOOTSTRAP_DEPENDENCY, fetchBootstrap } from '#lib/api/system.ts';

import type { LayoutLoad } from './$types';

export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ depends, fetch }) => {
  depends(BOOTSTRAP_DEPENDENCY);
  try {
    const bootstrap: Bootstrap = await fetchBootstrap(api, fetch);
    return { bootstrap, bootstrapError: null };
  } catch (error) {
    // The shell still renders and keeps retrying (see +layout.svelte).
    return { bootstrap: null, bootstrapError: error };
  }
};
