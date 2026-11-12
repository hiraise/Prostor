// The notification feature (NTF, plan step 1.10): the bell with the unread badge and the inbox
// popup (1.10b-1); the «Уведомления» section of the account page with subscriptions, and the
// subscription sections of the tag and saved-search dialogs (1.10b-2); the watcher popup of the
// issue page (1.10b-3).
export { default as NotificationBell } from './NotificationBell.svelte';
export { default as NotificationSettings } from './NotificationSettings.svelte';
export { default as SourceSubscription } from './SourceSubscription.svelte';
export { default as WatchersPopup } from './WatchersPopup.svelte';
export {
  followInbox,
  inboxSummary,
  listInbox,
  markInboxRead,
  onInboxRows,
  setInboxSummaryForTests,
  type InboxFolder,
  type InboxSource,
} from './inbox.svelte.ts';
