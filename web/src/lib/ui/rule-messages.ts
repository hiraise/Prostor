// What the built-in rules told the actor (AUT-023, §3.13): every message of a mutation's answer
// shows as a toast, its text chosen by code in the API layer (architecture §14). A message the
// caller can lead somewhere — a clone's copy — gains the toast's action button.
import { ruleMessageText } from '#lib/api/rules.ts';
import type { Message } from '#lib/api/schema.gen.ts';

import { toasts, type ToastAction } from './toast.svelte.ts';

/** Shows the rule messages of a mutation answer, one toast each. */
export function showRuleMessages(
  messages: readonly Message[] | undefined,
  actionFor?: (message: Message) => ToastAction | undefined,
): void {
  for (const message of messages ?? []) {
    toasts.show({ message: ruleMessageText(message), action: actionFor?.(message) });
  }
}
