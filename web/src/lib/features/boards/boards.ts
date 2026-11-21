// Boards in the data store (architecture.md §15) and the calls of the board lists: the entry and
// the list (BRD-001, BRD-002), the switcher and favorites, creating from a preset (BRD-003) and the
// addresses of boards (BRD-017). The view of a board (cells, cards) lives in its page's state.
import { resolve } from '$app/paths';

import { ApiError, api } from '#lib/api/client.ts';
import type {
  CellDraft,
  NewCellDraft,
  PlacedLane,
  BoardCards,
  BoardCardsInput,
  Board,
  BoardCell,
  BoardColumn,
  BoardColumnPatch,
  BoardImpact,
  BoardLanes,
  BoardLanesInput,
  BoardList,
  BoardPatch,
  BoardProfile,
  BoardProfilePatch,
  BoardSummary,
  BoardBacklog,
  BoardView,
  CardBatchAnswer,
  CardMove,
  IssueBoard,
  LaneMove,
  NewBoard,
  NewCard,
  NewSprint,
  NewBoardColumn,
  PageProject,
  PlacedCard,
  Sprint,
  SprintPatch,
} from '#lib/api/schema.gen.ts';
import { queryWord } from '#lib/features/issues/index.ts';
import { getLocale } from '#lib/i18n/locale.svelte.ts';
import { can, canAnywhere, currentUser } from '#lib/session/index.ts';
import { store, type EntityHandle } from '#lib/store/index.ts';
import { daysBetween, parseIsoDate, type CalendarDate } from '#lib/time/calendar.ts';
import { formatDuration } from '#lib/time/duration.ts';

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    board: Board;
  }
}

/** Boards per page of a list; a person sees tens, rarely hundreds. */
export const BOARDS_PAGE = 50;

store.defineKind('board', {
  async load(ids) {
    const answers = await Promise.all(
      ids.map((id) =>
        api
          .get('/boards/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    return answers.filter((board): board is Board => board !== null);
  },
});

/** The address of a board; with `list` the entry shows the list instead of the last board. */
export const boardHref = {
  board: (id: number) => resolve('/(app)/boards/[id]', { id: String(id) }),
  /** A sprint of a board (BRD-017). */
  sprint: (id: number, sprint: number) =>
    resolve('/(app)/boards/[id]/sprints/[sprint]', { id: String(id), sprint: String(sprint) }),
  /** The list itself: `/boards` alone opens the board visited last (BRD-001). */
  list: () => `${resolve('/(app)/boards')}?list`,
  entry: () => resolve('/(app)/boards'),
  create: () => resolve('/(app)/boards/new'),
  project: (key: string) => resolve('/(app)/projects/[key]/boards', { key }),
};

/** Which boards a list shows. */
export interface BoardFilter {
  /** Part of the name. */
  q?: string;
  /** Boards including this project (PRJ-016). */
  project?: number;
}

/** One page of the boards the person views: favorites first, then by name. Not kept in the store. */
export function findBoards(
  filter: BoardFilter = {},
  cursor?: string | null,
  signal?: AbortSignal,
): Promise<BoardList> {
  const q = filter.q?.trim() ?? '';
  return api.get('/boards', {
    query: {
      q: q === '' ? undefined : q,
      project: filter.project,
      limit: BOARDS_PAGE,
      cursor: cursor ?? undefined,
    },
    signal,
  });
}

/** Where `/boards` leads (BRD-001): the board visited last, else the list with its first page. */
export type BoardEntry = { kind: 'board'; id: number } | { kind: 'list'; first: BoardList };

export async function boardEntry(showList: boolean): Promise<BoardEntry> {
  const first = await findBoards();
  if (!showList && first.last_visited !== null) return { kind: 'board', id: first.last_visited };
  return { kind: 'list', first };
}

/** What the address of a board leads to. */
export type OpenedBoard = { kind: 'found'; board: EntityHandle<Board> } | { kind: 'missing' };

/** The board of an address segment; a hidden or deleted board is missing. */
export async function openBoard(segment: string): Promise<OpenedBoard> {
  if (!/^\d{1,15}$/.test(segment)) return { kind: 'missing' };
  try {
    const handle = await store.ensureEntity('board', Number(segment), { form: 'full' });
    return handle.current ? { kind: 'found', board: handle } : { kind: 'missing' };
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_found') return { kind: 'missing' };
    throw error;
  }
}

/** The stored row of a board, if loaded. Reactive. */
export function boardRow(id: number): Board | undefined {
  return store.entities.table('board').get(id);
}

/**
 * May create boards (BRD-003): a signed-in person who reads issues somewhere; the server checks
 * that they read issues in every chosen project. Reactive.
 */
export function canCreateBoards(): boolean {
  return currentUser() !== null && canAnywhere('issue.read');
}

/** Creates a board from a preset (BRD-003…BRD-007). */
export async function createBoard(input: NewBoard): Promise<Board> {
  const board = await api.post('/boards', { body: input });
  store.entities.table('board').upsert(board, 'full');
  return board;
}

/** Stars or unstars a board for the signed-in person (BRD-002); a loaded board row follows. */
export async function setBoardFavorite(id: number, favorite: boolean): Promise<void> {
  const row = store.entities.table('board').get(id);
  const before = row?.favorite;
  if (row) row.favorite = favorite;
  try {
    await api.patch('/boards/{id}/profile', { path: { id }, body: { favorite } });
  } catch (error) {
    const now = store.entities.table('board').get(id);
    if (now && before !== undefined) now.favorite = before;
    throw error;
  }
}

/** Favorites first, then by name, as the server orders them; for a list changed in place. */
export function orderBoards(boards: BoardSummary[]): BoardSummary[] {
  return [...boards].sort(
    (a, b) =>
      Number(b.favorite) - Number(a.favorite) ||
      a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }),
  );
}

/** Whether a name is taken by a board in the list, ignoring letter case and outer spaces. */
export function nameTaken(name: string, boards: readonly Pick<BoardSummary, 'name'>[]): boolean {
  const wanted = name.trim().toLocaleLowerCase();
  return wanted !== '' && boards.some((board) => board.name.trim().toLocaleLowerCase() === wanted);
}

/** At most this many projects join a board through «Все мои проекты». */
const MY_PROJECTS_MAX = 200;

/**
 * «Все мои проекты» of a new board (BRD-003): active projects whose team includes the person and
 * whose issues they read, up to 200.
 */
export async function myProjects(signal?: AbortSignal): Promise<PageProject['items']> {
  const found: PageProject['items'] = [];
  let cursor: string | undefined;
  do {
    const page = await api.get('/projects', {
      query: { in_teams: true, limit: 100, cursor },
      signal,
    });
    found.push(
      ...page.items.filter(
        (project) => project.archived_at === null && can('issue.read', project.id),
      ),
    );
    cursor = page.next_cursor ?? undefined;
  } while (cursor && found.length < MY_PROJECTS_MAX);
  return found.slice(0, MY_PROJECTS_MAX);
}

/** The view of a board (BRD-068): one sprint's cards, the column counts, the person's profile. */
export async function loadBoardView(
  id: number,
  sprint?: number | null,
  signal?: AbortSignal,
  q = '',
  lanes: string | null = null,
): Promise<BoardView> {
  const view = await api.get('/boards/{id}/view', {
    path: { id },
    query: { sprint: sprint ?? undefined, q: q === '' ? undefined : q, lanes: lanes ?? undefined },
    signal,
  });
  store.entities.table('board').upsert(view.board, 'full');
  return view;
}

/** The rest of a truncated cell (BRD-057), a page at a time. */
export function loadCell(
  id: number,
  column: number,
  sprint: number | null,
  cursor: string | null,
  q = '',
  lane: string | null = null,
): Promise<BoardCell> {
  return api.get('/boards/{id}/cell', {
    path: { id },
    query: {
      column,
      lane: lane ?? undefined,
      sprint: sprint ?? undefined,
      cursor: cursor ?? undefined,
      q: q === '' ? undefined : q,
    },
  });
}

/** Moves a card to a column and between neighbours (BRD-068, BRD-069). */
export function moveCard(
  id: number,
  sprint: number,
  issue: number,
  move: CardMove,
): Promise<PlacedCard> {
  return api.post('/boards/{id}/sprints/{sprint}/cards/{issue}/move', {
    path: { id, sprint, issue },
    body: move,
  });
}

/** An estimation sum (BRD-062) in the estimation field's terms: minutes as a duration. */
export function estimateText(board: Board, value: number): string {
  return board.cards.estimation?.kind === 'duration'
    ? formatDuration(value, getLocale())
    : String(Math.round(value * 100) / 100);
}

/** The lane settings as a change sends them (they replace the old ones whole). */
export function lanesInput(lanes: BoardLanes): BoardLanesInput {
  return {
    mode: lanes.mode,
    field: lanes.field?.id ?? null,
    attribute: lanes.attribute,
    values: lanes.values,
    other: lanes.other,
    new_card_value: lanes.new_card_value,
  };
}

/** The card settings as a change sends them whole (BRD-059…BRD-062). */
export function cardsInput(cards: BoardCards): BoardCardsInput {
  return {
    fields: cards.fields.map((one) => one.id),
    initials: cards.initials,
    color: cards.color,
    color_field: cards.color_field?.id ?? null,
    estimation: cards.estimation?.id ?? null,
    original_estimation: cards.original_estimation?.id ?? null,
  };
}

/** Puts a parent lane after or before another (BRD-053). */
export async function moveLane(
  id: number,
  sprint: number,
  issue: number,
  move: LaneMove,
): Promise<void> {
  await api.post('/boards/{id}/sprints/{sprint}/lanes/{issue}/move', {
    path: { id, sprint, issue },
    body: move,
  });
}

/** Takes a parent lane and its cards back to the backlog (BRD-056); the issues that left. */
export async function laneToBacklog(id: number, sprint: number, issue: number): Promise<number[]> {
  const removed = await api.delete('/boards/{id}/sprints/{sprint}/lanes/{issue}', {
    path: { id, sprint, issue },
  });
  return removed.issues;
}

/** Puts an issue on a sprint of the board (BRD-025): the undo of a removal. */
export function addCard(id: number, sprint: number, card: NewCard): Promise<PlacedCard> {
  return api.post('/boards/{id}/sprints/{sprint}/cards', { path: { id, sprint }, body: card });
}

/** The person's draft of a cell (BRD-077): the kept one, or a new card of the sprint. */
export function cellDraft(id: number, body: NewCellDraft): Promise<CellDraft> {
  return api.post('/boards/{id}/cell-draft', { path: { id }, body });
}

/** A new parent lane (BRD-054): an issue with this summary. */
export function createLane(id: number, sprint: number, title: string): Promise<PlacedLane> {
  return api.post('/boards/{id}/sprints/{sprint}/lanes', { path: { id, sprint }, body: { title } });
}

/** Takes a card off the sprint, or off a board without sprints (BRD-074). */
export async function removeCard(id: number, sprint: number, issue: number): Promise<void> {
  await api.delete('/boards/{id}/sprints/{sprint}/cards/{issue}', {
    path: { id, sprint, issue },
  });
}

/** Takes a tag off an issue (BRD-063). */
export async function untag(issue: number, tag: number): Promise<void> {
  await api.delete('/issues/{id}/tags/{tag_id}', { path: { id: issue, tag_id: tag } });
}

/** The boards and sprints holding an issue; for a draft also the boards it joins on its own. */
export async function issueBoards(issue: number, signal?: AbortSignal): Promise<IssueBoard[]> {
  return (await api.get('/issues/{id}/boards', { path: { id: issue }, signal })).items;
}

/** Puts an issue on a board (ISS-020): into `sprint`, else the board's current sprint. */
export async function addIssueBoard(
  issue: number,
  board: number,
  sprint?: number,
): Promise<IssueBoard[]> {
  return (await api.post('/issues/{id}/boards', { path: { id: issue }, body: { board, sprint } }))
    .items;
}

/** Takes an issue off a board; a draft is also kept off a board it would join on its own. */
export async function removeIssueBoard(issue: number, board: number): Promise<IssueBoard[]> {
  return (await api.delete('/issues/{id}/boards/{board}', { path: { id: issue, board } })).items;
}

/** Where these issues stand on the board now (the live board, BRD-114); at most 100. */
export function batchCards(
  id: number,
  issues: number[],
  sprint: number | null,
  q = '',
): Promise<CardBatchAnswer> {
  return api.post('/boards/{id}/cards/batch', {
    path: { id },
    body: { issues, sprint: sprint ?? undefined, q: q === '' ? undefined : q },
  });
}

/**
 * Shows an issue's field changes that would put it on a query board (BRD-028) and changes
 * nothing; a refusal (no change can make it match) says why.
 */
export function previewCard(id: number, sprint: number, card: NewCard): Promise<PlacedCard> {
  return addCard(id, sprint, { ...card, preview: true });
}

/** A page of the board's backlog (BRD-089, BRD-093): the issues ordered by hand first. */
export function loadBacklog(
  id: number,
  q: string,
  cursor: string | null,
  signal?: AbortSignal,
): Promise<BoardBacklog> {
  return api.get('/boards/{id}/backlog', {
    path: { id },
    query: { q: q.trim() === '' ? undefined : q, cursor: cursor ?? undefined },
    signal,
  });
}

/** Puts an issue after `after`, else before `before`, else first in the backlog (BRD-093). */
export async function orderBacklog(
  id: number,
  issue: number,
  after: number | null,
  before: number | null,
): Promise<void> {
  await api.post('/boards/{id}/backlog/order', {
    path: { id },
    body: {
      issue,
      ...(after !== null ? { after } : before !== null ? { before } : {}),
    },
  });
}

/** Forgets the backlog's order: the search's sort again (BRD-093). */
export async function discardBacklogOrder(id: number): Promise<void> {
  await api.delete('/boards/{id}/backlog/order', { path: { id } });
}

/** Keeps a board the server returned. */
function keepBoard(board: Board): Board {
  store.entities.table('board').upsert(board, 'full');
  return board;
}

/** Changes the settings of a board (BRD-013); a stale version answers `conflict`. */
export async function updateBoard(id: number, patch: BoardPatch): Promise<Board> {
  return keepBoard(await api.patch('/boards/{id}', { path: { id }, body: patch }));
}

/** Loads the board again, after a conflict. */
export async function reloadBoard(id: number): Promise<Board> {
  return keepBoard(await api.get('/boards/{id}', { path: { id } }));
}

/** What deleting a board takes with it (BRD-016). */
export function boardImpact(id: number): Promise<BoardImpact> {
  return api.get('/boards/{id}/impact', { path: { id } });
}

/** Deletes a board for everyone (BRD-016). */
export async function deleteBoard(id: number): Promise<void> {
  await api.delete('/boards/{id}', { path: { id } });
  store.entities.table('board').remove(id);
}

/** Changes the person's view of a board: card size, collapsed columns (BRD-047, BRD-058). */
export function setBoardProfile(id: number, patch: BoardProfilePatch): Promise<BoardProfile> {
  return api.patch('/boards/{id}/profile', { path: { id }, body: patch });
}

/** Adds a column: a value of the set, or a new value (BRD-043). */
export async function addColumn(id: number, column: NewBoardColumn): Promise<Board> {
  return keepBoard(await api.post('/boards/{id}/columns', { path: { id }, body: column }));
}

/** Moves a column, changes its limits or renames its value (BRD-044…BRD-046). */
export async function updateColumn(
  id: number,
  column: number,
  patch: BoardColumnPatch,
): Promise<Board> {
  return keepBoard(
    await api.patch('/boards/{id}/columns/{column}', { path: { id, column }, body: patch }),
  );
}

/** Removes a column; its cards leave the board and keep their sprint. */
export async function deleteColumn(id: number, column: number): Promise<Board> {
  return keepBoard(await api.delete('/boards/{id}/columns/{column}', { path: { id, column } }));
}

/** The sprints of a board, archived ones included, in creation order (BRD-083, BRD-103). */
export async function listSprints(id: number, signal?: AbortSignal): Promise<Sprint[]> {
  return (await api.get('/boards/{id}/sprints', { path: { id }, signal })).items;
}

/** Creates a sprint (BRD-098), optionally carrying the unresolved issues of another (BRD-101). */
export function createSprint(id: number, sprint: NewSprint): Promise<Sprint> {
  return api.post('/boards/{id}/sprints', { path: { id }, body: sprint });
}

/** Changes a sprint's name, goal and dates (BRD-102); a stale version answers `conflict`. */
export function updateSprint(id: number, sprint: number, patch: SprintPatch): Promise<Sprint> {
  return api.patch('/boards/{id}/sprints/{sprint}', { path: { id, sprint }, body: patch });
}

/** Deletes a sprint with its cards (BRD-104). */
export async function deleteSprint(id: number, sprint: number): Promise<void> {
  await api.delete('/boards/{id}/sprints/{sprint}', { path: { id, sprint } });
}

/** What can be done to a sprint by one call: start or finish by hand, archive, restore. */
export type SprintAction = 'start' | 'finish' | 'archive' | 'restore';

export function sprintAction(id: number, sprint: number, action: SprintAction): Promise<Sprint> {
  const path = { path: { id, sprint } };
  switch (action) {
    case 'start':
      return api.post('/boards/{id}/sprints/{sprint}/start', path);
    case 'finish':
      return api.post('/boards/{id}/sprints/{sprint}/finish', path);
    case 'archive':
      return api.post('/boards/{id}/sprints/{sprint}/archive', path);
    case 'restore':
      return api.post('/boards/{id}/sprints/{sprint}/restore', path);
  }
}

/** Carries the unresolved issues of a sprint to another (BRD-101). */
export function carrySprint(id: number, sprint: number, to: number): Promise<Sprint> {
  return api.post('/boards/{id}/sprints/{sprint}/carry', { path: { id, sprint }, body: { to } });
}

/**
 * Where a sprint stands on its schedule (BRD-083), in calendar days, weekends included:
 * - `none`: no dates;
 * - `ahead`: starts in `days`;
 * - `running`: `days` left after today; 0 is the last day;
 * - `open`: started, no end;
 * - `finished`: finished by hand or past its end.
 */
export type SprintPhase =
  | { kind: 'none' }
  | { kind: 'ahead'; days: number }
  | { kind: 'running'; days: number }
  | { kind: 'open' }
  | { kind: 'finished' };

export function sprintPhase(
  sprint: Pick<Sprint, 'starts_on' | 'ends_on' | 'finished'>,
  today: CalendarDate,
): SprintPhase {
  if (sprint.finished) return { kind: 'finished' };
  const starts = sprint.starts_on === null ? null : parseIsoDate(sprint.starts_on);
  const ends = sprint.ends_on === null ? null : parseIsoDate(sprint.ends_on);
  if (starts === null && ends === null) return { kind: 'none' };
  if (starts !== null && daysBetween(today, starts) > 0) {
    return { kind: 'ahead', days: daysBetween(today, starts) };
  }
  if (ends === null) return { kind: 'open' };
  const left = daysBetween(today, ends);
  return left < 0 ? { kind: 'finished' } : { kind: 'running', days: left };
}

/** Card sizes, smallest first (BRD-058). */
export const CARD_SIZES = [0, 1, 2, 3] as const;
export type CardSize = (typeof CARD_SIZES)[number];

/** A card size from the profile, kept within the four steps. */
export function cardSize(value: number): CardSize {
  return Math.min(3, Math.max(0, Math.round(value))) as CardSize;
}

/**
 * The issue list of one column («Открыть списком», §3.7): the board projects, the shown sprint of
 * a sprint board (`sprint: board/sprint`) and the column value.
 */
export function columnQuery(
  board: Board,
  column: BoardColumn,
  sprint: Sprint | null = null,
): string {
  const parts: string[] = [];
  if (board.projects.length > 0) {
    parts.push(`project: ${board.projects.map((one) => queryWord(one.project.key)).join(', ')}`);
  }
  if (board.sprints_on && sprint !== null && !sprint.internal) {
    parts.push(`sprint: ${queryWord(`${board.name}/${sprint.name}`)}`);
  }
  if (board.column_field) {
    parts.push(`${queryWord(board.column_field.name)}: ${queryWord(column.value.name)}`);
  }
  return parts.join(' ');
}

/** Plain text of the start of a description, for the largest cards: the marks of Markdown go. */
export function plainExcerpt(markdown: string): string {
  return markdown
    .replace(/```[\s\S]*?(```|$)/g, ' ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/^\s{0,3}(#{1,6}|>|[-*+]|\d+[.)])\s+/gm, '')
    .replace(/[*_~`]+/g, '')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '')
    .slice(0, 3)
    .join('\n');
}
