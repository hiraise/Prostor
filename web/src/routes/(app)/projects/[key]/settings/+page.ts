// The settings open on their general page.
import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';

import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => {
  redirect(307, resolve('/(app)/projects/[key]/settings/general', { key: params.key }));
};
