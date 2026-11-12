// The backlog of a board (BRD-089…BRD-096, D-045): its rows are a cached list of the
// store over `GET /boards/{id}/backlog` with the panel's filter, followed live like the issue list
// (D-043: rows in place, other changes offer «Обновить»). Reorders show at once and go to the
// server; a reorder heard from elsewhere moves the row in place when both rows are shown.
import { SvelteMap } from 'svelte/reactivity';

import type { BacklogMissing, BacklogSearch, Issue } from '#lib/api/schema.gen.ts';
import { keyText, store, type ListHandle } from '#lib/store/index.ts';

import { discardBacklogOrder, loadBacklog, orderBacklog } from '../boards.ts';

/** What the first page of a backlog said besides its rows. */
export interface BacklogMeta {
  search: BacklogSearch | null;
  missing: BacklogMissing | null;
  liveTopic: string | null;
}

const metas = new SvelteMap<string, BacklogMeta>();

function backlogKey(board: number, q: string): unknown[] {
  return ['board-backlog', { board, q: q.trim() }];
}

export class BacklogState {
  readonly board: number;
  /** The panel's filter: the board's header filter (BRD-081). */
  q = $state('');
  list = $state<ListHandle<Issue> | null>(null);
  /** The first load failed. */
  error = $state<unknown>(null);
  #asked = 0;

  constructor(board: number, q = '') {
    this.board = board;
    this.q = q;
  }

  /** What the server said about the backlog: its search, why there is none, its live topic. */
  get meta(): BacklogMeta | undefined {
    return metas.get(keyText(backlogKey(this.board, this.q)));
  }

  /** The rows in their order. */
  get ids(): readonly number[] {
    return this.list?.ids ?? [];
  }

  /** Shows the backlog with this filter; loads when it changed or was never loaded. */
  async show(q: string): Promise<void> {
    if (q === this.q && this.list) return;
    this.q = q;
    const asked = ++this.#asked;
    const board = this.board;
    const key = backlogKey(board, q);
    const text = keyText(key);
    try {
      const found = await store.ensureList('issue', {
        key,
        staleTimeMs: 0,
        fetch: async (cursor, signal) => {
          const answer = await loadBacklog(board, q, cursor, signal);
          if (cursor === null) {
            metas.set(text, {
              search: answer.search,
              missing: answer.missing,
              liveTopic: answer.page?.live_topic ?? null,
            });
          }
          return answer.page ?? { items: [], next_cursor: null };
        },
      });
      if (asked !== this.#asked) return;
      this.list = found;
      this.error = null;
    } catch (error) {
      if (asked === this.#asked) this.error = error;
    }
  }

  /** Loads the first page again; a failure keeps the rows and shows in `list.error`. */
  reload(): Promise<void> {
    // Not shown yet: the first `show` loads it; a failed first load tries again.
    if (!this.list) return this.error ? this.show(this.q) : Promise.resolve();
    return this.list.refresh().catch(() => undefined);
  }

  /** Takes a row off the panel (it went onto the board). */
  take(issue: number): void {
    this.list?.arrange(this.ids.filter((id) => id !== issue));
  }

  /**
   * Puts a row before the row now at `index` among the others (BRD-093): at once on the screen,
   * then on the server. A failure reloads the server's order and rethrows.
   */
  async order(issue: number, index: number): Promise<void> {
    const list = this.list;
    if (!list) return;
    const others = this.ids.filter((id) => id !== issue);
    const at = Math.min(Math.max(index, 0), others.length);
    const next = [...others.slice(0, at), issue, ...others.slice(at)];
    list.arrange(next);
    try {
      await orderBacklog(this.board, issue, others[at - 1] ?? null, others[at] ?? null);
    } catch (error) {
      await this.reload();
      throw error;
    }
  }

  /** A reorder heard live: moves the row in place when both are shown, else reloads. */
  moved(issue: number, after: number | null): void {
    const ids = this.ids;
    if (!ids.includes(issue) || (after !== null && !ids.includes(after))) {
      void this.reload();
      return;
    }
    const others = ids.filter((id) => id !== issue);
    const at = after === null ? 0 : others.indexOf(after) + 1;
    this.list?.arrange([...others.slice(0, at), issue, ...others.slice(at)]);
  }

  /** Forgets the manual order (BRD-093) and loads the search's order. */
  async discard(): Promise<void> {
    await discardBacklogOrder(this.board);
    await this.reload();
  }
}
