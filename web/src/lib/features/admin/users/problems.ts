// What a dialog says when the server refuses an action: the missing permissions by name (ACC-050),
// the action's own explanation of a conflict, or the text of the error code.
import { ApiError } from '#lib/api/client.ts';
import type { PermissionKey } from '#lib/api/schema.gen.ts';
import { permissions } from '#lib/features/access/index.ts';
import { describeError } from '#lib/ui/toast.svelte.ts';

import * as m from './messages.ts';

function isPermission(key: string): key is PermissionKey {
  return Object.hasOwn(permissions, key);
}

/**
 * The text of a refusal; `conflict` explains what a `conflict` answer means for this action. The
 * last-administrator guard has its own code and text.
 */
export function refusalText(error: unknown, conflict?: () => string): string {
  if (error instanceof ApiError) {
    const missing = error.body?.missing_permissions ?? [];
    if (error.code === 'forbidden' && missing.length > 0) {
      const names = missing.map((key) => (isPermission(key) ? permissions[key].name() : key));
      return m.missingPermissions({ names: names.join(', ') });
    }
    if (error.code === 'conflict' && conflict) return conflict();
    if (error.code === 'last_administrator') return m.lastAdministrator();
  }
  return describeError(error).message;
}
