// What a board page shows (BRD-068): one sprint's view with its cells as lists of issue IDs, the
// cards' issues in their compact form with what they name, and the person's profile (card size,
// collapsed columns and lanes). A cell is a column in a lane (BRD-049); lanes come a page at a
// time (BRD-057). Cards stay here, apart from the issue store (D-044).
import { SvelteMap } from 'svelte/reactivity';

import { ApiError } from '#lib/api/client.ts';
import { showRuleMessages } from '#lib/ui/rule-messages.ts';
import type {
  BoardCell,
  BoardColumn,
  BoardProfile,
  BoardProfilePatch,
  BoardView,
  Card,
  CardField,
  CardIssue,
  CardBatchAnswer,
  CardRefs,
  FieldRef,
  GroupRef,
  HeldValue,
  Lane,
  PlacedCard,
  TruncatedCell,
  UserRef,
  WipState,
} from '#lib/api/schema.gen.ts';
import { announce } from '#lib/ui/announce.svelte.ts';
import { describeError, showError, toasts } from '#lib/ui/toast.svelte.ts';

import { can } from '#lib/session/index.ts';

import {
  addCard,
  cellDraft,
  createLane,
  laneToBacklog,
  lanesInput,
  loadBoardView,
  loadCell,
  moveCard,
  moveLane,
  previewCard,
  removeCard,
  setBoardProfile,
  untag,
  updateBoard,
} from '../boards.ts';
import { openNewIssue } from '#lib/features/issues/index.ts';
import * as m from '../messages.ts';

/** A cell: the IDs of its shown cards in order, and how many it holds. */
export interface CellState {
  ids: number[];
  total: number;
  /** Where the next page starts; `null` when everything is shown. */
  cursor: string | null;
  loading: boolean;
}

/**
 * Where a card stands: its column, its lane, and its index among the shown cards of the cell. A
 * place without `lane` keeps the card's lane.
 */
export interface CardPlace {
  column: number;
  lane?: string | null;
  index: number;
}

/** The key of a cell: its column, and its lane on a board with lanes. */
export function cellKey(column: number, lane: string | null = null): string {
  return lane === null ? String(column) : `${String(column)}|${lane}`;
}

/** Whether a column holds as many cards as its limits ask (BRD-046). */
export function wipOf(column: Pick<BoardColumn, 'wip_min' | 'wip_max'>, count: number): WipState {
  if (column.wip_max !== null && count > column.wip_max) return 'over';
  if (column.wip_min !== null && count < column.wip_min) return 'under';
  return 'within';
}

/** Ranks sort as strings; a card without one goes last. */
function before(a: string | null, b: string | null): boolean {
  if (a === null) return false;
  return b === null || a < b;
}

/** A failure in our words; a refusal adds the server's reason (what rule stopped it). */
function failed(message: string, error: unknown): void {
  const refused =
    error instanceof ApiError && ['forbidden', 'invalid_request'].includes(error.code)
      ? error.body?.message
      : undefined;
  toasts.show({ tone: 'error', message, detail: refused ?? describeError(error).message });
}

export class BoardState {
  readonly id: number;
  view = $state<BoardView | null>(null);
  error = $state<unknown>(null);
  loading = $state(false);
  profile = $state<BoardProfile | null>(null);
  /** Cells by `cellKey`. */
  cells = $state<Record<string, CellState>>({});
  /** The lanes shown so far, in order (BRD-053); empty on a board without lanes. */
  lanes = $state<Lane[]>([]);
  /** Where the next page of lanes starts; `null` when every lane is shown. */
  lanesNext = $state<string | null>(null);
  lanesLoading = $state(false);
  readonly issues = new SvelteMap<number, CardIssue>();
  readonly cards = new SvelteMap<number, Card>();
  readonly fields = new SvelteMap<number, FieldRef>();
  readonly values = new SvelteMap<number, HeldValue>();
  readonly users = new SvelteMap<number, UserRef>();
  readonly groups = new SvelteMap<number, GroupRef>();
  /** The board is gone or no longer visible: the page shows the not-found state. */
  gone = $state(false);
  /** The sprint asked for (BRD-017); `null` shows the current one (BRD-099). */
  wanted = $state<number | null>(null);
  /** The header filter (BRD-081); empty shows every card. */
  query = $state('');
  /** A card is being dragged: live changes wait for the drop (§3.17). */
  dragging = $state(false);
  /** The issue shown in the popup (BRD-079), in edit mode (BRD-080). */
  popup = $state<{ issue: number; edit: boolean } | null>(null);
  #controller: AbortController | null = null;
  #waiting: (() => void)[] = [];

  constructor(id: number, sprint: number | null = null, query = '') {
    this.id = id;
    this.wanted = sprint;
    this.query = query;
  }

  /** Shows another sprint or filter; loads only when one of them changed. */
  show(sprint: number | null, query: string): Promise<void> {
    if (sprint === this.wanted && query === this.query && (this.view || this.loading)) {
      return Promise.resolve();
    }
    this.wanted = sprint;
    this.query = query;
    return this.load();
  }

  /** A sort clause in the header filter orders the cells (BRD-070): no reordering by hand. */
  get sorted(): boolean {
    return (this.view?.sort?.length ?? 0) > 0;
  }

  /** Loads the view; a failure keeps what was shown and sets `error`. */
  async load(): Promise<void> {
    this.#controller?.abort();
    const controller = new AbortController();
    this.#controller = controller;
    this.loading = true;
    try {
      const view = await loadBoardView(this.id, this.wanted, controller.signal, this.query);
      if (controller.signal.aborted) return;
      this.#apply(view);
      this.error = null;
    } catch (error) {
      if (controller.signal.aborted) return;
      this.error = error;
      if (error instanceof ApiError && error.status === 404) this.gone = true;
    } finally {
      if (this.#controller === controller) this.loading = false;
    }
  }

  #apply(view: BoardView): void {
    this.issues.clear();
    this.cards.clear();
    this.#keep(view.issues, view.refs);
    const cells: Record<string, CellState> = {};
    const lanes = view.board.lanes.mode === 'none' ? [null] : view.lanes.map((lane) => lane.key);
    this.#fill(cells, view, lanes);
    this.cells = cells;
    this.lanes = view.lanes;
    this.lanesNext = view.lanes_next;
    this.profile = view.profile;
    this.view = view;
  }

  /** Makes the cells of `lanes` with their cards; the sizes of truncated cells come with them. */
  #fill(
    cells: Record<string, CellState>,
    page: Pick<BoardView, 'columns' | 'cards' | 'cells'>,
    lanes: (string | null)[],
  ): void {
    for (const lane of lanes) {
      for (const column of page.columns) {
        cells[cellKey(column.column, lane)] = { ids: [], total: 0, cursor: null, loading: false };
      }
    }
    for (const card of page.cards) {
      const cell = cells[cellKey(card.column, card.lane)];
      if (!cell || !lanes.includes(card.lane)) continue;
      this.cards.set(card.issue, card);
      cell.ids.push(card.issue);
      cell.total += 1;
    }
    page.cells.forEach((truncated: TruncatedCell) => {
      if (!lanes.includes(truncated.lane)) return;
      const cell = cells[cellKey(truncated.column, truncated.lane)];
      if (!cell) return;
      cell.cursor = truncated.next_cursor;
      cell.total = truncated.total;
    });
  }

  /** Whether the board shows lanes (BRD-049). */
  get lanesOn(): boolean {
    return (this.view?.board.lanes.mode ?? 'none') !== 'none';
  }

  /** Shows the next page of lanes (BRD-057): the grid asks when its end comes into view. */
  async moreLanes(): Promise<void> {
    const cursor = this.lanesNext;
    if (cursor === null || this.lanesLoading || !this.view) return;
    this.lanesLoading = true;
    try {
      const page = await loadBoardView(this.id, this.wanted, undefined, this.query, cursor);
      if (this.lanesNext !== cursor) return;
      const fresh = page.lanes.filter((lane) => !this.lanes.some((one) => one.key === lane.key));
      this.#keep(page.issues, page.refs);
      this.#fill(
        this.cells,
        page,
        fresh.map((lane) => lane.key),
      );
      this.lanes.push(...fresh);
      this.lanesNext = page.lanes_next;
    } catch (error) {
      showError(new Error(m.moreLanesFailed(), { cause: error }), () => this.moreLanes());
    } finally {
      this.lanesLoading = false;
    }
  }

  /** The cell of a column in a lane. */
  cell(column: number, lane: string | null = null): CellState | undefined {
    return this.cells[cellKey(column, lane)];
  }

  #keep(issues: CardIssue[], refs: CardRefs): void {
    for (const field of refs.fields) this.fields.set(field.id, field);
    for (const value of refs.values) this.values.set(value.id, value);
    for (const user of refs.users) this.users.set(user.id, user);
    for (const group of refs.groups) this.groups.set(group.id, group);
    for (const issue of issues) this.#replace(issue);
  }

  /** Keeps a card's issue; a changed estimation of a shown card moves the sums (BRD-062). */
  #replace(issue: CardIssue): void {
    const known = this.issues.get(issue.id);
    const place = known ? this.placeOf(issue.id) : null;
    this.issues.set(issue.id, issue);
    if (!known || !place) return;
    const delta = this.estimateOf(issue) - this.estimateOf(known);
    if (delta !== 0) this.#sum(place.column, place.lane, delta);
  }

  /** The estimation of a card in the field's unit (minutes for durations); 0 without one. */
  estimateOf(issue: CardIssue | undefined): number {
    const field = this.view?.board.cards.estimation?.id;
    if (field === undefined || !issue) return 0;
    return issue.fields.find((one) => one.field === field)?.number ?? 0;
  }

  /** Adds to the estimation sums of a column, a lane and the board. */
  #sum(column: number, lane: string | null, by: number): void {
    const view = this.view;
    if (!view || by === 0) return;
    const count = view.columns.find((one) => one.column === column);
    if (count?.estimate != null) count.estimate += by;
    const header = lane === null ? undefined : this.lanes.find((one) => one.key === lane);
    if (header?.estimate != null) header.estimate += by;
    if (view.totals.estimate !== null) view.totals.estimate += by;
  }

  /**
   * Shows a card's field with a new value at once and saves it (BRD-063); `save` sends it. A
   * refusal brings the old value back and tells why.
   */
  async editField(
    issue: number,
    field: CardField,
    refs: Partial<CardRefs>,
    save: () => Promise<unknown>,
  ): Promise<void> {
    const before = this.issues.get(issue);
    if (!before) return;
    const name = this.fields.get(field.field)?.name ?? '';
    const empty = field.ids.length === 0 && field.text === null && field.number === null;
    const fields = before.fields.filter((one) => one.field !== field.field);
    if (!empty) fields.push(field);
    this.#keep([{ ...before, fields }], {
      fields: refs.fields ?? [],
      values: refs.values ?? [],
      users: refs.users ?? [],
      groups: refs.groups ?? [],
    });
    try {
      await save();
      announce(m.fieldSaved({ name }));
    } catch (error) {
      const now = this.issues.get(issue);
      if (now) this.#replace({ ...now, fields: before.fields });
      failed(m.fieldFailed({ name }), error);
    }
  }

  /** Takes a tag off a card's issue (BRD-063). */
  async removeTag(issue: number, tag: number): Promise<void> {
    const before = this.issues.get(issue);
    if (!before) return;
    const name = before.tags.find((one) => one.id === tag)?.name ?? '';
    this.issues.set(issue, { ...before, tags: before.tags.filter((one) => one.id !== tag) });
    try {
      await untag(issue, tag);
      announce(m.tagRemoved({ name }));
    } catch (error) {
      const now = this.issues.get(issue);
      if (now) this.issues.set(issue, { ...now, tags: before.tags });
      failed(m.tagFailed({ name }), error);
    }
  }

  /** Shows the next page of a truncated cell («Ещё N», BRD-057). */
  async more(column: number, lane: string | null = null): Promise<void> {
    const cell = this.cell(column, lane);
    if (!cell || cell.loading || cell.cursor === null) return;
    cell.loading = true;
    try {
      const page: BoardCell = await loadCell(
        this.id,
        column,
        this.view?.sprint?.id ?? null,
        cell.cursor,
        this.query,
        lane,
      );
      this.#keep(page.issues, page.refs);
      for (const card of page.cards) {
        this.cards.set(card.issue, card);
        if (!cell.ids.includes(card.issue)) cell.ids.push(card.issue);
      }
      cell.cursor = page.next_cursor;
    } catch (error) {
      showError(new Error(m.moreFailed(), { cause: error }), () => this.more(column, lane));
    } finally {
      cell.loading = false;
    }
  }

  /** Cards the cell holds but does not show yet. */
  hidden(column: number, lane: string | null = null): number {
    const cell = this.cell(column, lane);
    if (cell?.cursor == null) return 0;
    return Math.max(0, cell.total - cell.ids.length);
  }

  /** Changes the profile at once and saves it; a failure puts it back. */
  async setProfile(patch: BoardProfilePatch): Promise<void> {
    const before = this.profile;
    if (!before) return;
    this.profile = {
      favorite: patch.favorite ?? before.favorite,
      card_size: patch.card_size ?? before.card_size,
      collapsed_columns: patch.collapsed_columns ?? before.collapsed_columns,
      collapsed_lanes: patch.collapsed_lanes ?? before.collapsed_lanes,
    };
    try {
      this.profile = await setBoardProfile(this.id, patch);
    } catch (error) {
      this.profile = before;
      showError(new Error(m.profileFailed(), { cause: error }));
    }
  }

  /** Collapses or expands a column for this person (BRD-047). */
  toggleColumn(column: number): Promise<void> {
    const collapsed = this.profile?.collapsed_columns ?? [];
    return this.setProfile({
      collapsed_columns: collapsed.includes(column)
        ? collapsed.filter((id) => id !== column)
        : [...collapsed, column],
    });
  }

  /** Collapses or expands a lane for this person (BRD-053). */
  toggleLane(key: string): Promise<void> {
    const collapsed = this.profile?.collapsed_lanes ?? [];
    return this.setProfile({
      collapsed_lanes: collapsed.includes(key)
        ? collapsed.filter((one) => one !== key)
        : [...collapsed, key],
    });
  }

  /** Whether every shown lane is collapsed. */
  get allLanesCollapsed(): boolean {
    const collapsed = this.profile?.collapsed_lanes ?? [];
    return this.lanes.length > 0 && this.lanes.every((lane) => collapsed.includes(lane.key));
  }

  /** «Свернуть все» and «Развернуть все» (BRD-053): over the lanes shown so far. */
  collapseLanes(on: boolean): Promise<void> {
    const collapsed = this.profile?.collapsed_lanes ?? [];
    const keys = this.lanes.map((lane) => lane.key);
    return this.setProfile({
      collapsed_lanes: on
        ? [...collapsed, ...keys.filter((key) => !collapsed.includes(key))]
        : collapsed.filter((key) => !keys.includes(key)),
    });
  }

  /**
   * Whether a lane can go up (`-1`) or down (`1`), by an editor (BRD-053): parent lanes among
   * parent lanes, value lanes among the lanes the board keeps; «Прочее» stays where the settings
   * put it.
   */
  canMoveLane(key: string, step: -1 | 1): boolean {
    const board = this.view?.board;
    if (!board?.rights.edit || key === 'other' || this.sorted) return false;
    if (board.lanes.mode === 'parents') {
      if (this.sprint === null) return false;
      const index = this.lanes.findIndex((lane) => lane.key === key);
      const next = this.lanes[index + step];
      return index >= 0 && next !== undefined && next.issue !== null;
    }
    const order = this.#valueOrder();
    if (!order) return false;
    const index = order.indexOf(key);
    return index >= 0 && order[index + step] !== undefined;
  }

  /** The value lanes in the order the board keeps; date lanes kept empty show every range. */
  #valueOrder(): string[] | null {
    const lanes = this.view?.board.lanes;
    if (lanes?.mode !== 'values' || (lanes.field === null && lanes.attribute === null)) return null;
    if (lanes.values.length > 0) return lanes.values;
    if (this.lanesNext !== null) return null;
    return this.lanes.map((lane) => lane.key).filter((key) => key !== 'other');
  }

  /** Moves a lane up or down a place (BRD-053). */
  async moveLane(key: string, step: -1 | 1): Promise<void> {
    const view = this.view;
    if (!view || !this.canMoveLane(key, step)) return;
    const board = view.board;
    if (board.lanes.mode === 'parents') {
      const sprint = this.sprint;
      const index = this.lanes.findIndex((lane) => lane.key === key);
      const lane = this.lanes[index];
      const next = this.lanes[index + step];
      if (sprint === null || !lane?.issue || !next?.issue) return;
      const before = [...this.lanes];
      this.lanes.splice(index, 1);
      this.lanes.splice(index + step, 0, lane);
      try {
        await moveLane(
          this.id,
          sprint,
          lane.issue,
          step < 0 ? { before: next.issue } : { after: next.issue },
        );
      } catch (error) {
        this.lanes = before;
        failed(m.laneMoveFailed(), error);
      }
      return;
    }
    const order = [...(this.#valueOrder() ?? [])];
    const index = order.indexOf(key);
    const other = order[index + step];
    if (other === undefined) return;
    order[index] = other;
    order[index + step] = key;
    try {
      await updateBoard(this.id, {
        lanes: { ...lanesInput(board.lanes), values: order },
        version: board.version,
      });
      await this.load();
    } catch (error) {
      failed(m.laneMoveFailed(), error);
      void this.load();
    }
  }

  /** The person may take a parent lane back to the backlog (BRD-056). */
  canLaneToBacklog(lane: Lane): boolean {
    return (
      lane.issue !== null &&
      this.sprint !== null &&
      this.view?.board.mode !== 'query' &&
      this.issues.get(lane.issue)?.can_update === true
    );
  }

  /** Takes a parent lane and its cards off the sprint to the backlog (BRD-056). */
  async laneToBacklog(lane: Lane): Promise<void> {
    const sprint = this.sprint;
    if (lane.issue === null || sprint === null) return;
    try {
      const gone = await laneToBacklog(this.id, sprint, lane.issue);
      toasts.show({ tone: 'success', message: m.laneSentBack({ count: gone.length }) });
      await this.load();
    } catch (error) {
      failed(m.laneBackFailed(), error);
    }
  }

  /** A lane's title and color (BRD-053): our words for dates and «Прочее», else the name. */
  laneLabel(key: string): { name: string; color: number | null } {
    const plain = (name: string | undefined) => ({ name: name ?? m.hiddenLane(), color: null });
    if (key === 'other') return plain(m.otherLane());
    const [prefix = '', rest = ''] = key.split(':');
    if (prefix === 'd') return plain(m.dateLanes[rest]?.() ?? rest);
    const id = Number(rest);
    const board = this.view?.board;
    if (prefix === 'i') return plain(this.issues.get(id)?.summary);
    if (prefix === 'p')
      return plain(board?.projects.find((one) => one.project.id === id)?.project.name);
    const label = board?.lanes.labels.find((one) => one.key === key);
    if (label) return { name: label.name, color: label.color };
    if (prefix === 'v') {
      const held = this.values.get(id);
      return { name: held?.name ?? m.hiddenLane(), color: held?.color ?? null };
    }
    if (prefix === 'u') return plain(this.users.get(id)?.full_name);
    if (prefix === 'g') return plain(this.groups.get(id)?.name);
    return plain(undefined);
  }

  /** The field values of a card the way it names them. */
  valuesOf(field: CardField): string[] {
    const kind = this.fields.get(field.field)?.kind;
    if (kind === 'user') return field.ids.map((id) => this.users.get(id)?.full_name ?? '');
    if (kind === 'group') return field.ids.map((id) => this.groups.get(id)?.name ?? '');
    return field.ids.map((id) => this.values.get(id)?.name ?? '');
  }

  /** The sprint the board shows; `null` when a sprint board has no current one. */
  get sprint(): number | null {
    return this.view?.sprint?.id ?? null;
  }

  /** Opens an issue in the popup (BRD-079); `edit` starts in edit mode (BRD-080). */
  openIssue(issue: number, edit = false): void {
    this.popup = { issue, edit };
  }

  /** A cell draft is open in the new-issue dialog; the page reloads the view when it closes
   * (a publish sends no board event, so the new card would not appear otherwise). */
  awaitedDialog = false;

  /** Opens the new-issue dialog on a draft of the cell (BRD-077): the card lands in it. */
  async createInCell(column: number, lane: string | null): Promise<void> {
    const draft = await this.cellDraft(column, lane);
    if (draft === null) return;
    this.awaitedDialog = true;
    openNewIssue({ draft });
  }

  /** The person may create issues on the shown sprint: in some board project, not archived. */
  get canCreate(): boolean {
    const view = this.view;
    if (!view || this.sprint === null || view.sprint?.archived) return false;
    return view.board.projects.some(
      (one) => !one.project.archived && can('issue.create', one.project.id),
    );
  }

  /** Lanes are parent issues and the person may make one (BRD-054). */
  get canCreateLane(): boolean {
    return (
      this.view?.board.lanes.mode === 'parents' &&
      this.view.board.mode !== 'query' &&
      this.canCreate
    );
  }

  /** The person's draft of a cell (BRD-077); `null` when it could not be had. */
  async cellDraft(column: number, lane: string | null): Promise<number | null> {
    const sprint = this.sprint;
    if (sprint === null) return null;
    try {
      const draft = await cellDraft(this.id, {
        sprint,
        column,
        ...(lane === null ? {} : { lane }),
      });
      return draft.issue;
    } catch (error) {
      failed(m.cellDraftFailed(), error);
      return null;
    }
  }

  /** Makes a parent lane (BRD-054) and shows it; `false` when refused. */
  async createLane(title: string): Promise<boolean> {
    const sprint = this.sprint;
    if (sprint === null) return false;
    try {
      const placed = await createLane(this.id, sprint, title);
      announce(m.laneCreated({ name: placed.issue.summary }));
      await this.load();
      return true;
    } catch (error) {
      failed(m.laneCreateFailed(), error);
      return false;
    }
  }

  /** Where a shown card stands. */
  placeOf(issue: number): (CardPlace & { lane: string | null }) | null {
    const card = this.cards.get(issue);
    if (!card) return null;
    const index = this.cell(card.column, card.lane)?.ids.indexOf(issue) ?? -1;
    return index < 0 ? null : { column: card.column, lane: card.lane, index };
  }

  /** The person may move this card (the issue's update right, a shown sprint). */
  canMove(issue: number): boolean {
    return this.sprint !== null && this.issues.get(issue)?.can_update === true;
  }

  /** The person may take this card off: not on a query board, where the query decides. */
  canRemove(issue: number): boolean {
    return this.canMove(issue) && this.view?.board.mode !== 'query';
  }

  /** Starts or ends a drag; changes heard meanwhile run after the drop. */
  setDragging(on: boolean): void {
    this.dragging = on;
    if (on) return;
    const waiting = this.#waiting;
    this.#waiting = [];
    for (const run of waiting) run();
  }

  /** Runs `work` now, or after the drag when a card is being dragged. */
  whenStill(work: () => void): void {
    if (this.dragging) this.#waiting.push(work);
    else work();
  }

  /**
   * Moves a card (BRD-068, BRD-069): at once on the screen, then on the server. A failure puts it
   * back and says why; a success offers to undo, which moves it back between its old neighbours.
   */
  async move(issue: number, place: CardPlace, undoable = true): Promise<void> {
    const shown = this.issues.get(issue);
    const from = this.placeOf(issue);
    const sprint = this.sprint;
    if (!shown || !from || sprint === null) return;
    const to = { ...place, lane: place.lane === undefined ? from.lane : place.lane };
    const sameCell = from.column === to.column && from.lane === to.lane;
    if (sameCell && from.index === to.index) return;
    const sorted = this.sorted;
    if (sorted && sameCell) {
      toasts.show({ tone: 'info', message: m.sortedNoReorder() });
      return;
    }
    this.#put(issue, to);
    const ids = this.cell(to.column, to.lane)?.ids ?? [];
    const at = ids.indexOf(issue);
    const after = at > 0 ? ids[at - 1] : undefined;
    const next = ids[at + 1];
    const name = this.#columnName(to.column);
    try {
      const placed = await moveCard(this.id, sprint, issue, {
        column: to.column,
        ...(to.lane !== from.lane ? { lane: to.lane } : {}),
        version: shown.version,
        ...(this.query === '' ? {} : { q: this.query }),
        // A sorted cell places the card by the sort, not between neighbours.
        ...(sorted
          ? {}
          : after !== undefined
            ? { after }
            : next !== undefined
              ? { before: next }
              : {}),
      });
      if (sorted) void this.load();
      else this.#placed(issue, placed);
      showRuleMessages(placed.messages);
      announce(m.cardMovedTo({ id: shown.readable_id ?? String(issue), column: name }));
      if (undoable) {
        toasts.show({
          tone: 'success',
          message: m.cardMoved(),
          undo: () => this.move(issue, from, false),
        });
      }
    } catch (error) {
      if (this.cards.has(issue)) this.#put(issue, from);
      failed(m.moveFailed(), error);
      announce(m.cardMoveFailed({ id: shown.readable_id ?? String(issue) }), 'assertive');
    }
  }

  /**
   * Puts an issue on the shown sprint (BRD-096, BRD-025): in a column between its neighbours, or,
   * without a place, where its column value puts it. Resolves with the server's answer; a failure
   * says why and resolves with `null`. On a query board ask `preview` first (BRD-028).
   */
  async add(issue: number, to: CardPlace | null = null): Promise<PlacedCard | null> {
    const sprint = this.sprint;
    if (sprint === null) return null;
    const ids = to ? (this.cell(to.column, to.lane ?? null)?.ids ?? []) : [];
    const after = to && to.index > 0 ? ids[to.index - 1] : undefined;
    const next = to ? ids[to.index] : undefined;
    try {
      const placed = await addCard(this.id, sprint, {
        issue,
        ...(to ? { column: to.column } : {}),
        ...(to?.lane != null ? { lane: to.lane } : {}),
        // A sorted cell places the card by the sort.
        ...(this.sorted || !to
          ? {}
          : after !== undefined
            ? { after }
            : next !== undefined
              ? { before: next }
              : {}),
      });
      this.#placed(issue, placed);
      showRuleMessages(placed.messages);
      return placed;
    } catch (error) {
      failed(m.addFailed(), error);
      return null;
    }
  }

  /** What adding an issue to a query board would change (BRD-028); a refusal says why. */
  async preview(issue: number, to: CardPlace | null = null): Promise<PlacedCard | null> {
    const sprint = this.sprint;
    if (sprint === null) return null;
    try {
      return await previewCard(this.id, sprint, {
        issue,
        ...(to ? { column: to.column } : {}),
        ...(to?.lane != null ? { lane: to.lane } : {}),
      });
    } catch (error) {
      failed(m.queryRefused(), error);
      return null;
    }
  }

  /** Takes a card off the sprint or the board (BRD-074); the undo puts it back in its place. */
  async remove(issue: number): Promise<void> {
    const shown = this.issues.get(issue);
    const from = this.placeOf(issue);
    const sprint = this.sprint;
    if (!shown || !from || sprint === null) return;
    const ids = this.cell(from.column, from.lane)?.ids ?? [];
    const after = from.index > 0 ? ids[from.index - 1] : undefined;
    const card = this.cards.get(issue);
    this.forget(issue);
    const sprintsOn = this.view?.board.sprints_on === true;
    try {
      await removeCard(this.id, sprint, issue);
      toasts.show({
        tone: 'success',
        message: sprintsOn ? m.cardRemovedSprint() : m.cardRemovedBoard(),
        undo: async () => {
          try {
            const placed = await addCard(this.id, sprint, {
              issue,
              column: from.column,
              ...(from.lane !== null ? { lane: from.lane } : {}),
              ...(after !== undefined ? { after } : {}),
            });
            this.#placed(issue, placed);
          } catch (error) {
            failed(m.restoreFailed(), error);
          }
        },
      });
    } catch (error) {
      if (card) {
        this.cards.set(issue, card);
        this.#insert(issue, from.column, from.lane, from.index);
      }
      failed(m.removeFailed(), error);
    }
  }

  /** Drops a card from the screen (it left the board or the issue was deleted). */
  forget(issue: number): void {
    const place = this.placeOf(issue);
    this.cards.delete(issue);
    if (!place) return;
    const cell = this.cell(place.column, place.lane);
    if (!cell) return;
    cell.ids.splice(place.index, 1);
    this.#count(place.column, place.lane, -1);
    this.#sum(place.column, place.lane, -this.estimateOf(this.issues.get(issue)));
  }

  /** Moves a shown card right after another shown one (a live reorder); false when it cannot. */
  orderAfter(issue: number, after: number | null): boolean {
    const from = this.placeOf(issue);
    if (!from) return false;
    if (after === null) {
      this.#put(issue, { column: from.column, lane: from.lane, index: 0 });
      return true;
    }
    const target = this.placeOf(after);
    if (!target) return false;
    const index =
      target.column === from.column && target.lane === from.lane && from.index < target.index
        ? target.index
        : target.index + 1;
    this.#put(issue, { column: target.column, lane: target.lane, index });
    return true;
  }

  /** Places, moves or drops the cards a live batch answered for (BRD-114). */
  applyBatch(ids: number[], answer: CardBatchAnswer): void {
    this.#keep(answer.issues, answer.refs);
    ids.forEach((issue, index) => {
      const card = answer.cards[index] ?? null;
      this.#land(issue, card);
    });
  }

  /** Brings the screen in line with where the server says the card is now. */
  #placed(issue: number, placed: PlacedCard): void {
    this.#keep(placed.issue ? [placed.issue] : [], placed.refs);
    this.#land(issue, placed.card);
  }

  #land(issue: number, card: Card | null): void {
    const known = this.cards.get(issue);
    if (card === null) {
      this.forget(issue);
      return;
    }
    if (
      known?.column === card.column &&
      (known.lane ?? null) === (card.lane ?? null) &&
      known.rank === card.rank
    ) {
      this.cards.set(issue, card);
      return;
    }
    if (known) this.forget(issue);
    const cell = this.cell(card.column, card.lane);
    if (!cell) {
      // Its lane is not shown yet: only the counts change.
      this.#count(card.column, card.lane, 1);
      this.#sum(card.column, card.lane, this.estimateOf(this.issues.get(issue)));
      return;
    }
    const index = cell.ids.findIndex((id) => before(card.rank, this.cards.get(id)?.rank ?? null));
    if (index < 0 && cell.cursor !== null) {
      // It ranks among the cards not shown yet: only the count changes.
      this.#count(card.column, card.lane, 1);
      this.#sum(card.column, card.lane, this.estimateOf(this.issues.get(issue)));
      return;
    }
    this.cards.set(issue, card);
    this.#insert(issue, card.column, card.lane, index < 0 ? cell.ids.length : index);
  }

  /** Puts a shown card at a place; `index` counts the cell without it. */
  #put(issue: number, to: CardPlace & { lane: string | null }): void {
    const card = this.cards.get(issue);
    if (!card) return;
    this.forget(issue);
    this.cards.set(issue, { ...card, column: to.column, lane: to.lane });
    this.#insert(issue, to.column, to.lane, to.index);
  }

  #insert(issue: number, column: number, lane: string | null, index: number): void {
    const cell = this.cell(column, lane);
    if (!cell) return;
    cell.ids.splice(Math.min(Math.max(index, 0), cell.ids.length), 0, issue);
    this.#count(column, lane, 1);
    this.#sum(column, lane, this.estimateOf(this.issues.get(issue)));
  }

  /** Changes the counts of a cell, its lane and its column, and the column's limit state. */
  #count(column: number, lane: string | null, by: number): void {
    const cell = this.cell(column, lane);
    if (cell) cell.total = Math.max(0, cell.total + by);
    const header = lane === null ? undefined : this.lanes.find((one) => one.key === lane);
    if (header) header.count = Math.max(0, header.count + by);
    const view = this.view;
    const count = view?.columns.find((one) => one.column === column);
    const settings = view?.board.columns.find((one) => one.id === column);
    if (!count || !settings) return;
    count.count = Math.max(0, count.count + by);
    count.wip = wipOf(settings, count.count);
  }

  #columnName(column: number): string {
    return this.view?.board.columns.find((one) => one.id === column)?.value.name ?? '';
  }
}
