// Boards (BRD): the entry and the list, the switcher, creating from a preset, the board page, the
// `board` store kind. Other features and routes use boards only through this file.
export { default as BoardList } from './list/BoardList.svelte';
export { default as BoardPage } from './board/BoardPage.svelte';
export { default as BoardSwitcher } from './list/BoardSwitcher.svelte';
export { default as IssueBoards } from './issue/IssueBoards.svelte';
export { default as CreateBoardPage } from './list/CreateBoardPage.svelte';
export { default as NewBoardHost } from './list/NewBoardHost.svelte';
export * as boardMessages from './messages.ts';
export {
  boardEntry,
  boardHref,
  boardRow,
  canCreateBoards,
  findBoards,
  openBoard,
  type BoardEntry,
  type OpenedBoard,
} from './boards.ts';
export { boardAddress, headerFilter, underBoards } from './board/header-filter.svelte.ts';
export { newBoardHref, openNewBoard } from './list/open.svelte.ts';
