// Dragging cards on pointer events (BRD-068, BRD-069; D-044), no library. A mouse starts
// after 4 px; touch and pen after a 350 ms press that moved less than 8 px, so a swipe still
// scrolls natively. The dragged card is a copy in a fixed layer moved by `transform` once a frame;
// cell and card rectangles are measured at the start and after scrolling; the insertion point is
// the midpoint of the cards; a line shows it; edges scroll the grid and the page; Esc cancels.
//
// One listener on the grid serves every card (`data-card` on the card's item, `data-drop-column`
// and, on a board with lanes, `data-drop-lane` on each cell), so a board of 500 cards adds no effect per card. The backlog panel joins with
// `{@attach drag.panel}` (`data-drop-backlog` on its list, BRD-093, BRD-096, BRD-074): its rows
// reorder, go onto a cell, and cards dropped on it leave the board.

/** Where a dropped card goes: a column, a lane, and its index among the other cards of the cell. */
export interface DropTarget {
  column: number;
  lane: string | null;
  index: number;
}

export interface DragOptions {
  /** Whether this card (or, `fromBacklog`, this backlog row) may be dragged. */
  canDrag(issue: number, fromBacklog?: boolean): boolean;
  /** The card was dropped on another place. */
  drop(issue: number, target: DropTarget): void;
  /** A card or a backlog row was dropped on the backlog, before its row `index` (others' count). */
  toBacklog?(issue: number, index: number, fromBacklog: boolean): void;
  /** A backlog row was dropped on a cell. */
  fromBacklog?(issue: number, target: DropTarget): void;
  /** A drag began (`true`) or ended (`false`). */
  active?(on: boolean): void;
  /**
   * Whether the grid shows one column at a time (a phone): then the sides do not scroll the grid
   * step by step; holding the card at a side for `PAGE_HOLD_MS` turns to the next column.
   */
  paged?(): boolean;
}

export const MOUSE_SLOP = 4;
export const TOUCH_SLOP = 8;
export const LONG_PRESS_MS = 350;
export const PAGE_HOLD_MS = 600;
/** How close to an edge the pointer scrolls, and how far a frame scrolls. */
const EDGE = 48;
const STEP = 14;

interface Press {
  issue: number;
  element: HTMLElement;
  pointerId: number;
  touch: boolean;
  x: number;
  y: number;
  timer: ReturnType<typeof setTimeout> | null;
}

/** A cell, or the backlog (`column` `null`). */
interface CellBox {
  column: number | null;
  lane: string | null;
  element: HTMLElement;
  rect: DOMRect;
  cards: DOMRect[];
}

/** Where the pointer would drop: a cell or the backlog (`column` `null`). */
interface Spot {
  column: number | null;
  lane: string | null;
  index: number;
}

function sameCell(a: Pick<Spot, 'column' | 'lane'>, b: Pick<Spot, 'column' | 'lane'>): boolean {
  return a.column === b.column && a.lane === b.lane;
}

/** Controls that keep their own pointer handling. */
const INTERACTIVE = 'button, input, textarea, select, [role="menu"], [data-no-drag]';

export class BoardDrag {
  /** The issue whose card is being dragged. */
  issue = $state<number | null>(null);

  readonly #options: DragOptions;
  #root: HTMLElement | null = null;
  // Not shown anywhere, so a plain set (not a reactive one).
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  readonly #panels = new Set<HTMLElement>();
  #press: Press | null = null;
  #cells: CellBox[] = [];
  #origin: Spot | null = null;
  #target: Spot | null = null;
  #ghost: HTMLElement | null = null;
  #line: HTMLElement | null = null;
  #offset = { x: 0, y: 0 };
  #pointer = { x: 0, y: 0 };
  #frame: number | null = null;
  /** The side the card is held at in the one-column grid, and since when. */
  #hold: { side: -1 | 1; since: number } | null = null;
  #stop: (() => void)[] = [];

  constructor(options: DragOptions) {
    this.#options = options;
  }

  /** The attachment of the grid: `{@attach drag.root}`. */
  readonly root = (node: HTMLElement): (() => void) => {
    this.#root = node;
    const stop = this.#pressOn(node);
    return () => {
      stop();
      this.cancel();
      this.#root = null;
    };
  };

  /** The attachment of the backlog panel: `{@attach drag.panel}`. */
  readonly panel = (node: HTMLElement): (() => void) => {
    this.#panels.add(node);
    const stop = this.#pressOn(node);
    return () => {
      stop();
      if (this.#press && node.contains(this.#press.element)) this.cancel();
      this.#panels.delete(node);
    };
  };

  #pressOn(node: HTMLElement): () => void {
    const down = (event: PointerEvent) => {
      this.#down(event);
    };
    // A link or an image would start the browser's own drag and take the pointer away.
    const native = (event: DragEvent) => {
      if (event.target instanceof Element && event.target.closest('[data-card]')) {
        event.preventDefault();
      }
    };
    node.addEventListener('pointerdown', down);
    node.addEventListener('dragstart', native);
    return () => {
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('dragstart', native);
    };
  }

  /** Ends a press or a drag without dropping. */
  cancel(): void {
    this.#finish();
  }

  #down(event: PointerEvent): void {
    if (this.#press || !event.isPrimary || event.button !== 0) return;
    const target = event.target instanceof Element ? event.target : null;
    if (!target || target.closest(INTERACTIVE)) return;
    const element = target.closest<HTMLElement>('[data-card]');
    const issue = Number(element?.dataset.card);
    const fromBacklog = element?.closest('[data-drop-backlog]') != null;
    if (!element || !Number.isFinite(issue) || !this.#options.canDrag(issue, fromBacklog)) return;
    const touch = event.pointerType !== 'mouse';
    const press: Press = {
      issue,
      element,
      pointerId: event.pointerId,
      touch,
      x: event.clientX,
      y: event.clientY,
      timer: null,
    };
    this.#press = press;
    this.#pointer = { x: event.clientX, y: event.clientY };
    if (touch) {
      press.timer = setTimeout(() => {
        press.timer = null;
        this.#begin();
      }, LONG_PRESS_MS);
    }
    this.#listen(window, 'pointermove', (e) => {
      this.#move(e as PointerEvent);
    });
    this.#listen(window, 'pointerup', (e) => {
      this.#up(e as PointerEvent);
    });
    this.#listen(window, 'pointercancel', (e) => {
      if ((e as PointerEvent).pointerId === press.pointerId) this.#finish();
    });
    this.#listen(window, 'keydown', (e) => {
      if ((e as KeyboardEvent).key !== 'Escape' || this.issue === null) return;
      e.preventDefault();
      e.stopPropagation();
      this.#finish();
    });
  }

  #move(event: PointerEvent): void {
    const press = this.#press;
    if (press?.pointerId !== event.pointerId) return;
    this.#pointer = { x: event.clientX, y: event.clientY };
    if (this.issue === null) {
      const distance = Math.hypot(event.clientX - press.x, event.clientY - press.y);
      if (press.touch && distance > TOUCH_SLOP) this.#finish();
      else if (!press.touch && distance > MOUSE_SLOP) this.#begin();
      return;
    }
    event.preventDefault();
    this.#schedule();
  }

  #up(event: PointerEvent): void {
    const press = this.#press;
    if (press?.pointerId !== event.pointerId) return;
    const issue = this.issue;
    const target = this.#target;
    const origin = this.#origin;
    this.#finish();
    if (issue === null || !target) return;
    if (origin && sameCell(origin, target) && origin.index === target.index) return;
    const fromBacklog = origin !== null && origin.column === null;
    if (target.column === null) this.#options.toBacklog?.(issue, target.index, fromBacklog);
    else if (fromBacklog) this.#options.fromBacklog?.(issue, { ...target, column: target.column });
    else
      this.#options.drop(issue, { column: target.column, lane: target.lane, index: target.index });
  }

  #begin(): void {
    const press = this.#press;
    const root = this.#root;
    if (!press || !root) return;
    const { element } = press;
    this.issue = press.issue;
    this.#options.active?.(true);
    if (press.touch && 'vibrate' in navigator) navigator.vibrate(10);
    try {
      element.setPointerCapture(press.pointerId);
    } catch {
      // A pointer the browser no longer tracks; window listeners still follow it.
    }
    element.style.touchAction = 'none';
    element.style.opacity = '0.4';
    // Touch scrolling that began with the press stops while the card moves.
    this.#listen(
      document,
      'touchmove',
      (e) => {
        e.preventDefault();
      },
      { passive: false },
    );
    this.#listen(
      window,
      'scroll',
      () => {
        this.#measure();
        this.#schedule();
      },
      { capture: true, passive: true },
    );
    // The press ends on the ID link; a drag must not open the issue.
    this.#listen(
      window,
      'click',
      (e) => {
        e.preventDefault();
        e.stopPropagation();
      },
      { capture: true, once: true },
    );

    const rect = element.getBoundingClientRect();
    this.#offset = { x: press.x - rect.left, y: press.y - rect.top };
    this.#ghost = ghostOf(element, rect);
    this.#line = lineElement();
    document.body.append(this.#ghost, this.#line);
    this.#measure();
    const cell = element.closest<HTMLElement>('[data-drop-column], [data-drop-backlog]');
    const column = cell && 'dropBacklog' in cell.dataset ? null : Number(cell?.dataset.dropColumn);
    const cards = cell ? [...cell.querySelectorAll<HTMLElement>('[data-card]')] : [];
    const lane = cell?.dataset.dropLane ?? null;
    this.#origin =
      column === null || Number.isFinite(column)
        ? { column, lane, index: cards.indexOf(element) }
        : null;
    this.#schedule();
  }

  /** Measures the cells, the backlog and their cards, the dragged one left out. */
  #measure(): void {
    const root = this.#root;
    const source = this.#press?.element;
    const box = (element: HTMLElement, column: number | null): CellBox => ({
      column,
      lane: element.dataset.dropLane ?? null,
      element,
      rect: element.getBoundingClientRect(),
      cards: [...element.querySelectorAll<HTMLElement>('[data-card]')]
        .filter((card) => card !== source)
        .map((card) => card.getBoundingClientRect()),
    });
    const cells = root
      ? [...root.querySelectorAll<HTMLElement>('[data-drop-column]')].map((cell) =>
          box(cell, Number(cell.dataset.dropColumn)),
        )
      : [];
    for (const panel of this.#panels) {
      const lists = panel.matches('[data-drop-backlog]')
        ? [panel]
        : [...panel.querySelectorAll<HTMLElement>('[data-drop-backlog]')];
      for (const list of lists) cells.push(box(list, null));
    }
    this.#cells = cells;
  }

  #schedule(): void {
    this.#frame ??= requestAnimationFrame(() => {
      this.#frame = null;
      this.#render();
    });
  }

  #render(): void {
    if (this.issue === null) return;
    const { x, y } = this.#pointer;
    if (this.#ghost) {
      this.#ghost.style.transform = `translate3d(${String(x - this.#offset.x)}px, ${String(y - this.#offset.y)}px, 0)`;
    }
    this.#target = this.#hit(x, y);
    this.#drawLine();
    if (this.#scrollEdges(x, y)) {
      this.#measure();
      this.#schedule();
    }
  }

  /**
   * The backlog under the pointer, else the cell under it (or the nearest across), and the index
   * among its cards.
   */
  #hit(x: number, y: number): Spot | null {
    const backlog = this.#cells.find(
      (cell) =>
        cell.column === null &&
        x >= cell.rect.left &&
        x <= cell.rect.right &&
        y >= cell.rect.top &&
        y <= cell.rect.bottom,
    );
    if (backlog) {
      const index = backlog.cards.filter((card) => card.top + card.height / 2 < y).length;
      return { column: null, lane: null, index };
    }
    let best: CellBox | null = null;
    let distance = Infinity;
    for (const cell of this.#cells) {
      if (cell.column === null) continue;
      const across =
        x < cell.rect.left ? cell.rect.left - x : x > cell.rect.right ? x - cell.rect.right : 0;
      // Lanes stack cells: the distance up and down counts too.
      const down =
        cell.lane === null
          ? 0
          : y < cell.rect.top
            ? cell.rect.top - y
            : y > cell.rect.bottom
              ? y - cell.rect.bottom
              : 0;
      const away = across + down;
      if (away < distance) {
        best = cell;
        distance = away;
      }
    }
    if (!best) return null;
    const index = best.cards.filter((card) => card.top + card.height / 2 < y).length;
    return { column: best.column, lane: best.lane, index };
  }

  #drawLine(): void {
    const line = this.#line;
    const target = this.#target;
    const cell = target ? this.#cells.find((one) => sameCell(one, target)) : undefined;
    if (!line) return;
    if (!target || !cell) {
      line.hidden = true;
      return;
    }
    const above = cell.cards[target.index - 1];
    const below = cell.cards[target.index];
    const top =
      above && below
        ? (above.bottom + below.top) / 2
        : above
          ? above.bottom + 4
          : below
            ? below.top - 4
            : cell.rect.top + 8;
    line.hidden = false;
    line.style.transform = `translate3d(${String(cell.rect.left + 8)}px, ${String(top - 1)}px, 0)`;
    line.style.width = `${String(Math.max(0, cell.rect.width - 16))}px`;
  }

  /** Scrolls the grid sideways and the grid or the page up and down near their edges. */
  #scrollEdges(x: number, y: number): boolean {
    const root = this.#root;
    if (!root) return false;
    const box = root.getBoundingClientRect();
    let moved = false;
    // Over the backlog only its list scrolls.
    if (this.#target?.column === null) {
      const list = this.#cells.find((cell) => cell.column === null);
      if (!list) return false;
      const { element, rect } = list;
      const step = y < rect.top + EDGE ? -STEP : y > rect.bottom - EDGE ? STEP : 0;
      const before = element.scrollTop;
      if (step !== 0) element.scrollBy(0, step);
      return element.scrollTop !== before;
    }
    const scroll = (element: Element, left: number, top: number) => {
      const before = [element.scrollLeft, element.scrollTop];
      // A scroll that cannot move is left out: it would stop the turn to the next column.
      const room =
        (left < 0 && element.scrollLeft > 0) ||
        (left > 0 && element.scrollLeft + element.clientWidth < element.scrollWidth) ||
        (top < 0 && element.scrollTop > 0) ||
        (top > 0 && element.scrollTop + element.clientHeight < element.scrollHeight);
      if (!room) return;
      element.scrollBy(left, top);
      if (element.scrollLeft !== before[0] || element.scrollTop !== before[1]) moved = true;
    };
    const side = x < box.left + EDGE ? -1 : x > box.right - EDGE ? 1 : 0;
    if (this.#options.paged?.()) {
      this.#turn(root, side);
    } else if (side !== 0) {
      scroll(root, side * STEP, 0);
    }
    const page = document.scrollingElement ?? document.documentElement;
    if (y < Math.max(box.top, 0) + EDGE) {
      scroll(root, 0, -STEP);
      scroll(page, 0, -STEP);
    } else if (y > Math.min(box.bottom, window.innerHeight) - EDGE) {
      scroll(root, 0, STEP);
      scroll(page, 0, STEP);
    }
    return moved;
  }

  /** Turns the one-column grid to the next column after the card was held at a side. */
  #turn(root: HTMLElement, side: -1 | 0 | 1): void {
    if (side === 0) {
      this.#hold = null;
      return;
    }
    const now = performance.now();
    if (this.#hold?.side !== side) this.#hold = { side, since: now };
    if (now - this.#hold.since >= PAGE_HOLD_MS) {
      this.#hold = { side, since: now };
      root.scrollBy({ left: side * root.clientWidth, behavior: 'smooth' });
    }
    // Frames keep coming while the card waits at the side.
    this.#frame ??= requestAnimationFrame(() => {
      this.#frame = null;
      this.#render();
    });
  }

  #listen(
    target: EventTarget,
    type: string,
    listener: (event: Event) => void,
    options?: AddEventListenerOptions,
  ): void {
    target.addEventListener(type, listener, options);
    this.#stop.push(() => {
      target.removeEventListener(type, listener, options);
    });
  }

  #finish(): void {
    const press = this.#press;
    const wasActive = this.issue !== null;
    if (press?.timer) clearTimeout(press.timer);
    if (this.#frame !== null) cancelAnimationFrame(this.#frame);
    this.#frame = null;
    this.#hold = null;
    // The click that ends a drag comes right after `pointerup`; its blocker goes a moment later.
    const stop = this.#stop;
    this.#stop = [];
    const blocker = wasActive ? stop.splice(stop.length - 1, 1) : [];
    for (const run of stop) run();
    if (blocker.length > 0) setTimeout(() => blocker[0]?.(), 0);
    if (press) {
      press.element.style.touchAction = '';
      press.element.style.opacity = '';
    }
    this.#ghost?.remove();
    this.#line?.remove();
    this.#ghost = null;
    this.#line = null;
    this.#press = null;
    this.#cells = [];
    this.#origin = null;
    this.#target = null;
    if (wasActive) {
      this.issue = null;
      this.#options.active?.(false);
    }
  }
}

/** A copy of the card that follows the pointer; hidden from assistive technology. */
function ghostOf(element: HTMLElement, rect: DOMRect): HTMLElement {
  const ghost = element.cloneNode(true) as HTMLElement;
  for (const named of ghost.querySelectorAll('[id]')) named.removeAttribute('id');
  ghost.removeAttribute('data-card');
  ghost.setAttribute('aria-hidden', 'true');
  ghost.inert = true;
  ghost.dataset.dragGhost = '';
  Object.assign(ghost.style, {
    position: 'fixed',
    left: '0',
    top: '0',
    width: `${String(rect.width)}px`,
    margin: '0',
    listStyle: 'none',
    pointerEvents: 'none',
    zIndex: '1000',
    opacity: '0.95',
    rotate: '2deg',
    boxShadow: 'var(--shadow-lg)',
    willChange: 'transform',
  });
  return ghost;
}

/** The line that shows where the card lands. */
function lineElement(): HTMLElement {
  const line = document.createElement('div');
  line.dataset.dropLine = '';
  line.setAttribute('aria-hidden', 'true');
  line.hidden = true;
  Object.assign(line.style, {
    position: 'fixed',
    left: '0',
    top: '0',
    height: '2px',
    borderRadius: '1px',
    background: 'var(--color-focus)',
    pointerEvents: 'none',
    zIndex: '1001',
  });
  return line;
}
