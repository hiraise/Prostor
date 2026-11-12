// The settings tab's requests (NTF-017, NTF-021–NTF-024, NTF-094; plan step 1.10 §3.1, §4.2):
// the person's notification settings, their subscriptions on tags and saved searches, and a test
// letter to their own address. Every kind list follows §3.1: a tag takes all seven subscribable
// kinds, a saved search none of the tag kinds, and `system` is never subscribable.
import { api } from '#lib/api/client.ts';
import type {
  EmailSent,
  NotificationKind,
  NotificationSettings,
  NotificationSubscription,
  SubscriptionSourceKind,
} from '#lib/api/schema.gen.ts';

export const TAG_KINDS: readonly NotificationKind[] = [
  'created',
  'updated',
  'resolved',
  'comment',
  'vote',
  'tag_added',
  'tag_removed',
];

export const SEARCH_KINDS: readonly NotificationKind[] = [
  'created',
  'updated',
  'resolved',
  'comment',
  'vote',
];

/** Which kinds a source may deliver (NTF-022). */
export function kindsOf(source: SubscriptionSourceKind): readonly NotificationKind[] {
  return source === 'tag' ? TAG_KINDS : SEARCH_KINDS;
}

/** The keys of the built-in watch subscription's kinds in the settings (NTF-017). */
export const WATCH_KINDS = [
  'watch_created',
  'watch_updated',
  'watch_resolved',
  'watch_comment',
  'watch_vote',
  'watch_tag',
] as const satisfies (keyof NotificationSettings)[];

export type WatchKind = (typeof WATCH_KINDS)[number];

/** The auto-watch switches (ISS-030): what makes their owner a watcher of an issue. */
export const AUTOWATCH_KEYS = [
  'autowatch_created',
  'autowatch_commented',
  'autowatch_assigned',
  'autowatch_voted',
] as const satisfies (keyof NotificationSettings)[];

export type AutoWatchKey = (typeof AUTOWATCH_KEYS)[number];

export function getSettings(): Promise<NotificationSettings> {
  return api.get('/users/me/notifications');
}

export function putSettings(next: NotificationSettings): Promise<NotificationSettings> {
  return api.put('/users/me/notifications', { body: next });
}

export async function listSubscriptions(): Promise<NotificationSubscription[]> {
  return (await api.get('/notifications/subscriptions')).subscriptions;
}

export function createSubscription(
  source: SubscriptionSourceKind,
  sourceId: number,
  kinds: NotificationKind[],
): Promise<NotificationSubscription> {
  return api.post('/notifications/subscriptions', {
    body: { source, source_id: sourceId, kinds },
  });
}

export function setSubscriptionKinds(
  id: number,
  kinds: NotificationKind[],
): Promise<NotificationSubscription> {
  return api.put('/notifications/subscriptions/{id}', { path: { id }, body: { kinds } });
}

export function deleteSubscription(id: number): Promise<void> {
  return api.delete('/notifications/subscriptions/{id}', { path: { id } });
}

/** The person's subscription on one source, if any. */
export function findSubscription(
  subscriptions: readonly NotificationSubscription[],
  source: SubscriptionSourceKind,
  sourceId: number,
): NotificationSubscription | null {
  return (
    subscriptions.find(
      (subscription) => subscription.source === source && subscription.source_id === sourceId,
    ) ?? null
  );
}

/** Sends a test letter to the caller's own address (NTF-007's own part, NTF-094). */
export function sendTestEmail(): Promise<EmailSent> {
  return api.post('/users/me/test-email');
}
