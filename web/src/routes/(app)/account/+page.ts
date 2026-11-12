import { ensureTokens } from '#lib/features/account/index.ts';
import { ensureAccount } from '#lib/features/access/index.ts';
import { currentUser } from '#lib/session/index.ts';

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
  const { bootstrap } = await parent();
  const user = currentUser();
  const [tokens, account] = await Promise.all([
    ensureTokens(),
    user ? ensureAccount(user.id) : Promise.resolve(null),
  ]);
  return {
    tokens,
    account,
    emailEnabled: bootstrap?.email_enabled ?? false,
    gravatar: bootstrap?.gravatar ?? false,
  };
};
