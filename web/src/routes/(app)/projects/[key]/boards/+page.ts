// The project's boards (PRJ-016): boards that include the project.
import { findBoards } from '#lib/features/boards/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  const { project } = await parent();
  const id = project?.current?.id;
  return { first: id === undefined ? null : await findBoards({ project: id }) };
};
