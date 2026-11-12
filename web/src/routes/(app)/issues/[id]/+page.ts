// An issue's page (ISS-001…ISS-003, UI-006): the address holds its readable ID, a former one or the
// internal ID; a missing or hidden issue shows "not found". One request loads the whole page.
import { openIssue } from '#lib/features/issues/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, params, url }) => {
  await parent();
  // `?comment=ID` opens the activity around that comment (CNT-007).
  const comment = Number(url.searchParams.get('comment'));
  const opened = await openIssue(
    params.id,
    Number.isSafeInteger(comment) && comment > 0 ? comment : undefined,
  );
  if (opened.kind === 'missing') return { issue: null, page: null, requested: params.id };
  return { issue: opened.issue, page: opened.page, requested: params.id };
};
