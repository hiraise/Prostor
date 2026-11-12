// The inbox's shared state (NTF-048, plan step 1.10 §4.1): the unread counts behind the bell's
// badge and the live follow of the person's `inbox:` topic. The topic brings every change — a row
// arrived (the badge counts it, the open popup reloads its page) or the read state changed (the
// counts are set, not added) — so nothing refetches on events; a reload happens on follow and
// resync, and the read requests answer the new counts themselves.
import type { Inbox, InboxSummary, LiveEvent, NotificationRead } from '#lib/api/schema.gen.ts';
import { api } from '#lib/api/client.ts';
import type { LiveConnection } from '#lib/live/connection.ts';
import { liveConnection } from '#lib/store/index.ts';

/** What follows the topic: the shared connection in the app, a stand-in in tests. */
export type InboxSource = Pick<LiveConnection, 'subscribe'>;

export type InboxFolder = 'all' | 'direct' | 'subscriptions' | 'system';

export interface InboxPageQuery {
  folder?: InboxFolder;
  unread?: 'only';
  before?: number;
  issue?: number;
  limit?: number;
}

let summaryState: InboxSummary | null = $state(null);

// Listeners, not state: the popup hears that rows may have arrived or left.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const rowListeners = new Set<() => void>();

/** The unread counts of the folders, `null` before the first load. Reactive. */
export function inboxSummary(): InboxSummary | null {
  return summaryState;
}

/** For tests: set or clear the counts directly. */
export function setInboxSummaryForTests(summary: InboxSummary | null): void {
  summaryState = summary;
}

/** Hears that rows may have changed while the caller shows them; stops with the returned function. */
export function onInboxRows(listener: () => void): () => void {
  rowListeners.add(listener);
  return () => {
    rowListeners.delete(listener);
  };
}

/** One page of the inbox: thread heads with their latest rows, or one issue's whole thread. */
export async function listInbox(query: InboxPageQuery = {}): Promise<Inbox> {
  return api.get('/notifications', { query });
}

/**
 * Marks rows read or unread (NTF-050) and keeps the badge in step: the request answers the new
 * unread counts, so no second call is needed.
 */
export async function markInboxRead(change: NotificationRead): Promise<void> {
  summaryState = await api.post('/notifications/read', { body: change });
}

/** Which folders one row belongs to, the badge's rules (the server's `unread_summary` counts). */
function countsOf(kind: string, reasons: readonly unknown[]): Omit<InboxSummary, 'total'> {
  const names = reasons.filter((reason): reason is string => typeof reason === 'string');
  return {
    direct: names.includes('mention') ? 1 : 0,
    subscriptions: names.some((reason) => reason.startsWith('tag:') || reason.startsWith('search:'))
      ? 1
      : 0,
    system: kind === 'system' ? 1 : 0,
  };
}

/** A row arrived: every folder count grows by the row's membership. */
function rowArrived(kind: string, reasons: readonly unknown[]): void {
  const add = countsOf(kind, reasons);
  const current = summaryState ?? { direct: 0, subscriptions: 0, system: 0, total: 0 };
  summaryState = {
    total: current.total + 1,
    direct: current.direct + add.direct,
    subscriptions: current.subscriptions + add.subscriptions,
    system: current.system + add.system,
  };
  for (const listener of rowListeners) listener();
}

function handleEvent(event: LiveEvent): void {
  if (event.entity.kind !== 'inbox') return;
  const patch = event.patch;
  if (patch === null || typeof patch !== 'object' || typeof patch.kind !== 'string') return;
  if (patch.kind === 'read') {
    const { total, direct, subscriptions, system } = patch;
    if (
      typeof total === 'number' &&
      typeof direct === 'number' &&
      typeof subscriptions === 'number' &&
      typeof system === 'number'
    ) {
      summaryState = { direct, subscriptions, system, total };
    }
  } else if (patch.kind === 'notification') {
    const { event: kind, reasons } = patch;
    if (typeof kind === 'string' && Array.isArray(reasons)) rowArrived(kind, reasons);
  }
}

export interface FollowInboxOptions {
  connection?: InboxSource;
  /** Loads the unread counts; tests may replace it. */
  load?: () => Promise<InboxSummary>;
}

function quietly(work: Promise<unknown>): void {
  // A failed load keeps what the badge has; the next event or page load tries again.
  work.catch(() => undefined);
}

/** Sets the counts from a load. */
function adopt(summary: Promise<InboxSummary>): void {
  quietly(summary.then((loaded) => (summaryState = loaded)));
}

/**
 * Follows the person's `inbox:` topic until the returned function is called (plan §3.6): only
 * they may, so the server refuses every other `inbox:` topic. The first counts load with the
 * follow; a reconnect replays what was missed, and resync loads the counts again.
 */
export function followInbox(userId: number, options: FollowInboxOptions = {}): () => void {
  const connection = options.connection ?? liveConnection();
  const load = options.load ?? (async () => api.get('/notifications/summary'));
  let subscribedBefore = false;
  adopt(load());
  const stop = connection.subscribe(`inbox:${String(userId)}`, {
    event: (event) => {
      handleEvent(event);
    },
    subscribed: () => {
      // Replayed events keep the counts; a reload after a reconnect makes sure of it.
      if (subscribedBefore) adopt(load());
      subscribedBefore = true;
    },
    resync: () => {
      adopt(load());
      for (const listener of rowListeners) listener();
    },
  });
  return () => {
    stop();
  };
}
