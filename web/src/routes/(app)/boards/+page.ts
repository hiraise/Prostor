// The boards entry (BRD-001): the board visited last, else the list; `?list` shows the list.
import { redirect } from '@sveltejs/kit';

import { boardEntry, boardHref } from '#lib/features/boards/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url }) => {
  await parent();
  const entry = await boardEntry(url.searchParams.has('list'));
  if (entry.kind === 'board') redirect(307, boardHref.board(entry.id));
  return { first: entry.first };
};
