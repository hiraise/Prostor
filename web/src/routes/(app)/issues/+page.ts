// The issue list (QRY-053, QRY-069, QRY-073): the address holds the query and the context; the
// rows load through the store, so a return to a list shows it at once and reloads it quietly.
import { ensureSearch, parseListUrl, resolveContext } from '#lib/features/issues/index.ts';
import { preferences } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url }) => {
  await parent();
  const params = parseListUrl(url);
  const [context] = await Promise.all([
    resolveContext(params.context).catch(() => null),
    preferences.ensure(),
  ]);
  // A refused query still shows the page, with its problems.
  let error: unknown = null;
  const list = await ensureSearch({
    q: params.q,
    context: context ?? { kind: 'all' },
    hideResolved: preferences.value('list_hide_resolved'),
  }).catch((failure: unknown) => {
    error = failure;
    return null;
  });
  return { q: params.q, context, list, error };
};
