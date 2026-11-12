// A board (BRD-017): `/boards/{id}` shows its current sprint. A missing or hidden board shows
// "not found".
import { openBoard } from '#lib/features/boards/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, params }) => {
  await parent();
  const opened = await openBoard(params.id);
  return { board: opened.kind === 'found' ? opened.board : null };
};
