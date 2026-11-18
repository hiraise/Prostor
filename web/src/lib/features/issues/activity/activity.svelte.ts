// The activity stream of the open issue (HIST-001, HIST-008…HIST-011, CNT-007): its loaded items,
// the cursors at both edges, the tab choice and the order from preferences, and the comment a deep
// link asked for. Items are kept oldest first; the order only changes how they are shown. Comments
// also go into the store, so their cards follow edits made elsewhere.
import { api } from '#lib/api/client.ts';
import type { ActivityItem, ActivityPage, Comment, HistoryItem } from '#lib/api/schema.gen.ts';
import { preferences } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

/** Items per page of the stream. */
export const ACTIVITY_PAGE = 50;

export type ChangeItem = Extract<ActivityItem, { type: 'change' }>;
export type CommentEntry = Extract<ActivityItem, { type: 'comment' }>;

/** What one row of the stream shows: a comment with the changes made right after it, or a change. */
export type StreamRow =
  | { type: 'comment'; id: number; comment: Comment; changes: HistoryItem[] }
  | { type: 'change'; id: number; change: HistoryItem };

/** Which of the stream's tabs is open: everything, only comments, only change records. */
export type ActivityView = 'all' | 'comments' | 'changes';

/** The person's tab choice from the two stored filter settings (legacy «both off» reads as «все»). */
export function viewQuery(): ActivityView {
  const comments = preferences.value('activity_comments');
  const changes = preferences.value('activity_changes');
  if (comments === changes) return 'all';
  return comments ? 'comments' : 'changes';
}

/** The query of a stream request for the person's tab choice. */
export function filterQuery(): { comments: boolean; changes: boolean } {
  const view = viewQuery();
  return { comments: view !== 'changes', changes: view !== 'comments' };
}

/** The first page in the person's order: the oldest items for oldest first, else the newest. */
export function firstPageQuery(aroundComment?: number) {
  const query = { limit: ACTIVITY_PAGE, ...filterQuery() };
  if (aroundComment !== undefined) return { ...query, around_comment: aroundComment };
  return preferences.value('activity_order') === 'oldest_first'
    ? { ...query, direction: 'newer' as const }
    : query;
}

/** Items of a page, oldest first: pages come newest first, except those going `newer`. */
function chronological(page: ActivityPage, direction: 'older' | 'newer'): ActivityItem[] {
  return direction === 'newer' ? page.items : [...page.items].reverse();
}

function putComments(items: ActivityItem[]): void {
  const comments = items.flatMap((item) => (item.type === 'comment' ? [item.comment] : []));
  if (comments.length > 0) store.put('comment', comments, 'full');
}

export class ActivityStream {
  /** Oldest first. */
  items = $state<ActivityItem[]>([]);
  /** Cursor for older items; `null` when the oldest is loaded. */
  older = $state<number | null>(null);
  /** Cursor for newer items; `null` when the newest is loaded. */
  newer = $state<number | null>(null);
  loading = $state<'older' | 'newer' | 'all' | null>(null);
  /** The comment a deep link asked for (CNT-007); it is scrolled to and highlighted once. */
  focus = $state<number | null>(null);

  /** Newer items that came while the stream's end was off screen (ISS-048). */
  incoming = $state.raw<ActivityItem[]>([]);
  /** Whether the stream's newest end is on screen; the view keeps it. */
  atEnd = true;
  /** A live catch-up asked for during a load. */
  #again: 'append' | 'wait' | null = null;

  readonly issueId: number;

  constructor(
    issueId: number,
    page: ActivityPage,
    /** The comment of a deep link; its page came by `around_comment`. */
    focus?: number,
  ) {
    this.issueId = issueId;
    this.focus = focus ?? null;
    const query = firstPageQuery(focus);
    this.accept(page, 'direction' in query ? query.direction : 'older');
  }

  readonly order = $derived(preferences.value('activity_order'));
  readonly view = $derived(viewQuery());

  /** The rows in the person's order, changes made right after a shown comment inside it (HIST-011). */
  readonly rows = $derived.by((): StreamRow[] => {
    const showComments = this.view !== 'changes';
    const showChanges = this.view !== 'comments';
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local to one computation
    const shown = new Set(
      this.items.flatMap((item) => (item.type === 'comment' ? [item.comment.id] : [])),
    );
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local to one computation
    const grouped = new Map<number, HistoryItem[]>();
    const rows: StreamRow[] = [];
    for (const item of this.items) {
      if (item.type === 'comment') {
        if (!showComments) continue;
        const changes: HistoryItem[] = [];
        grouped.set(item.comment.id, changes);
        rows.push({ type: 'comment', id: item.id, comment: item.comment, changes });
      } else if (showChanges) {
        const host = item.after_comment;
        if (showComments && host !== null && shown.has(host)) {
          // A record comes after its comment in time, so the comment's row is already there.
          grouped.get(host)?.push(item);
        } else {
          rows.push({ type: 'change', id: item.id, change: item });
        }
      }
    }
    return this.order === 'oldest_first' ? rows : rows.reverse();
  });

  /** Replaces the items with a first page. */
  private accept(page: ActivityPage, direction: 'older' | 'newer'): void {
    this.items = chronological(page, direction);
    this.incoming = [];
    this.older = page.older;
    this.newer = page.newer;
  }

  private request(query: Record<string, unknown>): Promise<ActivityPage> {
    return api.get('/issues/{id}/activity', {
      path: { id: this.issueId },
      query: { limit: ACTIVITY_PAGE, ...filterQuery(), ...query },
    });
  }

  /** Loads the page beyond one edge (HIST-010). */
  async more(direction: 'older' | 'newer'): Promise<void> {
    const from = direction === 'older' ? this.older : this.newer;
    if (from === null || this.loading) return;
    this.loading = direction;
    try {
      const page = await this.request({ from, direction });
      const items = chronological(page, direction);
      putComments(items);
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local to one computation
      const known = new Set(this.items.map((item) => item.id));
      const fresh = items.filter((item) => !known.has(item.id));
      if (direction === 'older') {
        this.items = [...fresh, ...this.items];
        this.older = page.older;
      } else {
        this.items = [...this.items, ...fresh];
        this.newer = page.newer;
      }
    } finally {
      this.loading = null;
    }
  }

  /**
   * Appends what came after the newest loaded item (live updates), unless a gap of unloaded newer
   * items is left to «Показать более новые».
   */
  appendNewer(): Promise<void> {
    return this.catchUp(true);
  }

  /**
   * Fetches what came after the newest loaded or waiting item (ISS-048): appended when `append`,
   * else kept in `incoming` for «Показать». A call during a load runs again after it, so no event
   * is missed.
   */
  async catchUp(append: boolean): Promise<void> {
    if (this.newer !== null) return;
    if (this.loading) {
      this.#again = append ? 'append' : (this.#again ?? 'wait');
      return;
    }
    const last = this.incoming.at(-1) ?? this.items.at(-1);
    this.loading = 'newer';
    try {
      const page = await this.request(
        last ? { from: last.id, direction: 'newer' } : { direction: 'newer' },
      );
      putComments(page.items);
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local to one computation
      const known = new Set([...this.items, ...this.incoming].map((item) => item.id));
      const fresh = page.items.filter((item) => !known.has(item.id));
      if (append) {
        this.items = [...this.items, ...this.incoming, ...fresh];
        this.incoming = [];
        this.newer = page.newer;
      } else if (fresh.length > 0) {
        this.incoming = [...this.incoming, ...fresh];
      }
    } finally {
      this.loading = null;
      const again = this.#again;
      this.#again = null;
      if (again) void this.catchUp(again === 'append' || this.atEnd).catch(() => undefined);
    }
  }

  /** Shows the items that came while the end was away (ISS-048). */
  showIncoming(): void {
    this.items = [...this.items, ...this.incoming];
    this.incoming = [];
  }

  /**
   * Shows a comment the person just posted: appended when the newest items are loaded, else the
   * first page is loaded again. Posted from the «Изменения» tab, the stream returns to «Все» so
   * the comment is seen.
   */
  async posted(): Promise<void> {
    if (this.view === 'changes') {
      await this.setView('all');
      return;
    }
    if (this.newer === null) await this.appendNewer();
    else await this.reload();
  }

  /** Loads the first page again, for new filters or a new order. */
  async reload(): Promise<void> {
    this.loading = 'all';
    try {
      const query = firstPageQuery();
      const direction = 'direction' in query ? query.direction : 'older';
      const page = await this.request(query);
      putComments(page.items);
      this.accept(page, direction);
    } finally {
      this.loading = null;
    }
  }

  /** Opens one of the stream's tabs (HIST-008); the choice follows the person to other devices. */
  async setView(view: ActivityView): Promise<void> {
    preferences.set({
      activity_comments: view !== 'changes',
      activity_changes: view !== 'comments',
    });
    await this.reload();
  }

  /** Sets the order (HIST-009); a stream loaded whole is only turned around. */
  async setOrder(order: 'oldest_first' | 'newest_first'): Promise<void> {
    preferences.set({ activity_order: order });
    if (this.older !== null || this.newer !== null) await this.reload();
  }
}
