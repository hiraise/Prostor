// The live board (BRD-114, BRD-115; D-044): the page follows `board:<id>`. Issue events
// gather for 100 ms and ask the board where those cards stand now; a reorder with both cards shown
// moves the card at once; settings and sprint changes reload the view quietly; missed events that
// cannot be replayed reload it too. While a card is being dragged, changes wait for the drop.
import type { LiveEvent } from '#lib/api/schema.gen.ts';
import type { LiveConnection } from '#lib/live/connection.ts';
import { liveConnection, liveSync } from '#lib/store/index.ts';

import { batchCards } from '../boards.ts';
import type { BoardState } from './board-state.svelte.ts';

export type LiveSource = Pick<LiveConnection, 'subscribe'>;

/** How long issue events gather before one request asks for their cards. */
export const BATCH_MS = 100;
/** The most issues one batch request takes. */
const BATCH_MAX = 100;

interface OrderPatch {
  issue: number;
  after: number | null;
  sprint: number;
}

function orderPatch(patch: LiveEvent['patch']): OrderPatch | null {
  if (!patch) return null;
  const { issue, after, sprint } = patch;
  if (typeof issue !== 'number' || typeof sprint !== 'number') return null;
  if (after !== null && typeof after !== 'number') return null;
  return { issue, after, sprint };
}

/** A reorder of the board's backlog (BRD-093): `search` instead of `sprint`. */
function backlogPatch(patch: LiveEvent['patch']): { issue: number; after: number | null } | null {
  if (!patch || typeof patch.search !== 'number') return null;
  const { issue, after } = patch;
  if (typeof issue !== 'number') return null;
  if (after !== null && typeof after !== 'number') return null;
  return { issue, after };
}

function quietly(work: Promise<unknown>): void {
  // A failed refresh keeps what the board shows; the next event tries again.
  work.catch(() => undefined);
}

export class BoardLive {
  readonly #model: BoardState;
  readonly #batchMs: number;
  // Not shown anywhere, so a plain set (not a reactive one).
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  readonly #pending = new Set<number>();
  #timer: ReturnType<typeof setTimeout> | null = null;
  /** Hears reorders of the backlog (the panel, when shown). */
  backlogOrder: ((issue: number, after: number | null) => void) | null = null;

  constructor(model: BoardState, batchMs = BATCH_MS) {
    this.#model = model;
    this.#batchMs = batchMs;
  }

  /** Follows the board's topic until the returned function is called. */
  follow(topic: string, connection: LiveSource = liveConnection()): () => void {
    const stop = liveSync.follow(connection, topic, {
      event: (event) => {
        this.#model.whenStill(() => {
          this.#handle(event);
        });
      },
      resync: () => {
        this.#model.whenStill(() => {
          quietly(this.#model.load());
        });
      },
      // Refused once the board is deleted or no longer shared with the person.
      denied: () => {
        this.#model.gone = true;
      },
    });
    return () => {
      stop();
      if (this.#timer !== null) clearTimeout(this.#timer);
      this.#timer = null;
      this.#pending.clear();
    };
  }

  #handle(event: LiveEvent): void {
    const { kind, id } = event.entity;
    const model = this.#model;
    switch (kind) {
      case 'issue':
        if (event.change === 'deleted') {
          this.#pending.delete(id);
          model.forget(id);
        } else this.#queue(id);
        return;
      case 'card_order': {
        const backlog = backlogPatch(event.patch);
        if (backlog) {
          this.backlogOrder?.(backlog.issue, backlog.after);
          return;
        }
        const patch = orderPatch(event.patch);
        if (!patch) {
          this.#queue(id);
          return;
        }
        if (patch.sprint !== model.sprint) return;
        if (this.#pending.has(patch.issue) || !model.orderAfter(patch.issue, patch.after)) {
          this.#queue(patch.issue);
        }
        return;
      }
      case 'board':
        if (event.change === 'deleted') model.gone = true;
        else quietly(model.load());
        return;
      case 'sprint':
        quietly(model.load());
        return;
      default:
    }
  }

  #queue(issue: number): void {
    this.#pending.add(issue);
    this.#timer ??= setTimeout(() => {
      this.#timer = null;
      this.#model.whenStill(() => {
        this.#flush();
      });
    }, this.#batchMs);
  }

  #flush(): void {
    const ids = [...this.#pending];
    this.#pending.clear();
    const model = this.#model;
    for (let start = 0; start < ids.length; start += BATCH_MAX) {
      const chunk = ids.slice(start, start + BATCH_MAX);
      quietly(
        batchCards(model.id, chunk, model.sprint, model.query).then((answer) => {
          model.whenStill(() => {
            model.applyBatch(chunk, answer);
          });
        }),
      );
    }
  }
}
