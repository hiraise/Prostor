// A project's pages (PRJ-014, PRJ-016). The address holds the project's key; a former key or the
// numeric ID leads to the current key (PRJ-005). A missing or hidden project shows "not found".
import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';

import { openProject } from '#lib/features/projects/index.ts';

import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ parent, params, url }) => {
  await parent();
  const opened = await openProject(params.key);
  if (opened.kind === 'moved') {
    const base = resolve('/(app)/projects/[key]', { key: params.key });
    const rest = url.pathname.startsWith(base) ? url.pathname.slice(base.length) : '';
    redirect(307, `${resolve('/(app)/projects/[key]', { key: opened.key })}${rest}${url.search}`);
  }
  return { project: opened.kind === 'found' ? opened.project : null };
};
