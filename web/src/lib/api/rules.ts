// The text of a built-in rule's message (AUT-023, §3.11): a mutation's answer carries `messages`,
// one per thing a rule did; the text is chosen by code — the params are already names, resolved by
// the store. Pure on purpose: the UI layer turns the texts into toasts (§3.13).
import * as texts from '#lib/i18n/messages/rules.ts';

import type { Message } from './schema.gen.ts';

/** What one message of a mutation answer reads as. */
export function ruleMessageText(message: Message): string {
  const params = message.params;
  switch (message.code) {
    case 'assignee_not_visible':
      return texts.assigneeNotVisible({ assignee: params.assignee ?? '' });
    case 'vote_added':
      return texts.voteAdded();
    case 'assigned_to_owner':
      return texts.assignedToOwner({ assignee: params.assignee ?? '', value: params.value ?? '' });
    case 'owner_not_assignable':
      return texts.ownerNotAssignable({ owner: params.owner ?? '', value: params.value ?? '' });
    case 'parent_set':
      return texts.parentSet({ issue: params.issue ?? '', value: params.value ?? '' });
    case 'parent_reopened':
      return texts.parentReopened({ issue: params.issue ?? '' });
    case 'link_reattached':
      return texts.linkReattached({ issue: params.issue ?? '' });
    case 'priority_raised':
      return texts.priorityRaised({ value: params.value ?? '' });
    case 'clone_created':
      return texts.cloneCreated({ issue: params.issue ?? '' });
    case 'draft_saved':
      return texts.draftCopied({ issue: params.issue ?? '' });
    default:
      return texts.ruleRan();
  }
}
