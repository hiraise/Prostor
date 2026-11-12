// A sprint of a board (BRD-017): `/boards/{id}/sprints/{sprint}`. A missing board shows "not
// found"; a missing sprint too, once the view answers.
import { openBoard } from '#lib/features/boards/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, params }) => {
  await parent();
  const sprint = /^\d{1,15}$/.test(params.sprint) ? Number(params.sprint) : null;
  const opened = sprint === null ? null : await openBoard(params.id);
  return { board: opened?.kind === 'found' ? opened.board : null, sprint };
};
