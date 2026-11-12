<script lang="ts">
  // The board page (BRD-068): the header with the switcher, the sticky toolbar with the sprint
  // controls (BRD-083) and the header filter (BRD-081), the columns with their cards, the footer
  // (BRD-084) and the settings panel (BRD-086). A board created a moment ago opens with its
  // settings (`?settings`). `/boards/{id}` shows the current sprint, `/boards/{id}/sprints/{s}` a
  // chosen one (BRD-017); `?q=` is the header filter, kept across boards (BRD-070). The backlog
  // panel (BRD-089…BRD-096) sits beside the columns, a sheet on phones; cards drag between them.
  // «Новая» makes a card, a lane, a sprint or a board (BRD-076); cards open in the issue popup
  // (BRD-079); a notice above the columns lists what is wrong with the settings (BRD-023).
  import Columns3 from '@lucide/svelte/icons/columns-3';
  import { onMount, untrack } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';

  import { goto } from '$app/navigation';

  import type { Board, Issue, Sprint } from '#lib/api/schema.gen.ts';
  import { QueryBar } from '#lib/features/issues/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { readStored, writeStored } from '#lib/storage.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
  import { NARROW } from '#lib/theme/breakpoints.ts';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import {
    boardHref,
    cardSize,
    sprintAction,
    type CardSize,
    type SprintAction,
  } from '../boards.ts';
  import { BacklogState } from '../backlog/backlog.svelte.ts';
  import BacklogPanel from '../backlog/BacklogPanel.svelte';
  import { changeLines, type ChangeLine } from '../backlog/query-mode.ts';
  import QueryModeDialog from '../backlog/QueryModeDialog.svelte';
  import { BoardDrag, type DropTarget } from '../drag/drag.svelte.ts';
  import BoardSwitcher from '../list/BoardSwitcher.svelte';
  import { openNewBoard } from '../list/open.svelte.ts';
  import * as m from '../messages.ts';
  import IssuePopup from '../popup/IssuePopup.svelte';
  import NewLaneDialog from '../popup/NewLaneDialog.svelte';
  import type { SettingsTab } from '../settings/settings.ts';
  import SettingsPanel from '../settings/SettingsPanel.svelte';
  import FinishDialog from '../sprints/FinishDialog.svelte';
  import SprintDialog from '../sprints/SprintDialog.svelte';
  import SprintMenu from '../sprints/SprintMenu.svelte';
  import SprintSchedule from '../sprints/SprintSchedule.svelte';
  import BoardFooter from './BoardFooter.svelte';
  import { BoardState, cellKey } from './board-state.svelte.ts';
  import BoardGrid from './BoardGrid.svelte';
  import BoardToolbar from './BoardToolbar.svelte';
  import HealthNotice from './HealthNotice.svelte';
  import { boardAddress, headerFilter } from './header-filter.svelte.ts';
  import { BoardLive, type LiveSource } from './live.svelte.ts';
  import { BoardZoom, type Zoom } from './zoom.svelte.ts';

  interface Props {
    board: Board;
    /** Opens the settings panel at the start (a new board). */
    openSettings?: boolean;
    /** Where live changes come from; the app's connection by default (tests pass their own). */
    live?: LiveSource;
    /** The sprint of the address; `null` shows the current one. */
    sprint?: number | null;
    /** The header filter of the address; `null` when it has none. */
    q?: string | null;
    /** The current time, for tests of the schedule. */
    now?: number;
  }

  let { board, openSettings = false, live, sprint = null, q = null, now }: Props = $props();

  // The address's filter, else the one kept from another board.
  const query = $derived(q ?? headerFilter.value);
  let chosen = $derived(sprint);
  $effect(() => {
    if (q !== null) headerFilter.set(q);
  });

  const model = $derived(
    new BoardState(
      board.id,
      untrack(() => chosen),
      untrack(() => query),
    ),
  );
  let settingsOpen = $state(untrack(() => openSettings && board.rights.edit));
  let settingsTab = $state<SettingsTab>('general');
  let laneOpen = $state(false);
  /** The board itself shows these instead of cards. */
  const noColumns = $derived(board.column_field === null || board.columns.length === 0);

  function openSettingsOn(tab: SettingsTab): void {
    settingsTab = tab;
    settingsOpen = true;
  }

  /** «Новая» → «Карточку»: the create form in the first column, in «Прочее» with lanes. */
  function newCard(): void {
    const column = board.columns[0];
    if (!column) return;
    const lane = model.lanesOn
      ? (model.lanes.find((one) => one.key === 'other') ?? model.lanes[0])?.key
      : null;
    if (lane === undefined) return;
    model.composing = cellKey(column.id, lane);
  }

  const createItems = $derived([
    ...(model.canCreate && !noColumns ? [{ label: m.newCardItem(), onSelect: newCard }] : []),
    ...(model.canCreateLane ? [{ label: m.newLaneItem(), onSelect: () => (laneOpen = true) }] : []),
    ...(board.sprints_on && board.rights.edit
      ? [{ label: m.newSprintItem(), onSelect: () => openSprint(null) }]
      : []),
    { label: m.createBoardItem(), onSelect: () => openNewBoard() },
  ]);

  // Loads once per board, sprint and filter; what the load reads and writes must not run it again.
  $effect(() => {
    const shown = model;
    const wanted = chosen;
    const filter = query;
    untrack(() => {
      void shown.show(wanted, filter).then(() => {
        if (filter !== '' && shown.error && shown.view) {
          showError(new Error(m.filterFailed(), { cause: shown.error }));
        }
      });
    });
  });

  // The filter changes the address in place: no new history entry, focus and scroll stay.
  const IN_PLACE = { replace: true, reset: false } as const;

  // A filter kept from another board joins the address.
  onMount(() => {
    if (q === null && headerFilter.value !== '') {
      // eslint-disable-next-line svelte/no-navigation-without-resolve -- boardAddress() resolves
      void goto(boardAddress(board.id, sprint, headerFilter.value), IN_PLACE);
    }
  });

  function search(text: string): void {
    headerFilter.set(text);
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- boardAddress() resolves
    void goto(boardAddress(board.id, chosen, headerFilter.value), IN_PLACE);
  }

  function chooseSprint(one: Sprint | null): void {
    chosen = one?.id ?? null;
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- boardAddress() resolves
    void goto(boardAddress(board.id, chosen, query));
  }

  // Sprint dialogs (BRD-098, BRD-101…BRD-104).
  let sprintOpen = $state(false);
  let editing = $state<Sprint | null>(null);
  let deleting = $state(false);
  let finishOpen = $state(false);

  function openSprint(one: Sprint | null, toDelete = false): void {
    editing = one;
    deleting = toDelete;
    sprintOpen = true;
  }

  const shownSprint = $derived(model.view?.sprint ?? null);
  const sprints = $derived(model.view?.sprints ?? []);
  // The current sprint, whose unresolved issues a new sprint may take.
  const carryFrom = $derived.by(() => {
    const current = sprints.find((one) => one.current && !one.internal) ?? null;
    if (!current || current.id !== shownSprint?.id) return current;
    const open =
      [...model.issues.values()].some((one) => one.resolved_at === null) ||
      (model.view?.cells.length ?? 0) > 0;
    return open ? current : null;
  });

  async function act(one: Sprint, action: SprintAction): Promise<void> {
    try {
      await sprintAction(board.id, one.id, action);
      if (action === 'start') toasts.show({ tone: 'success', message: m.sprintStarted() });
      if (action === 'restore') toasts.show({ tone: 'success', message: m.sprintRestored() });
      await model.load();
    } catch (error) {
      showError(new Error(m.sprintActionFailed(), { cause: error }));
    }
  }

  // Follows the board's topic; again when its projects change, so the gateway hears of them.
  const follower = $derived(new BoardLive(model));
  const topic = $derived(model.view?.live_topic ?? null);
  const subscription = $derived(
    topic === null ? null : { topic, projects: board.projects.map((one) => one.project.id) },
  );
  $effect(() => {
    const shown = follower;
    const followed = subscription?.topic;
    if (followed === undefined) return;
    return untrack(() => shown.follow(followed, live));
  });

  const size = $derived(cardSize(model.profile?.card_size ?? 1));
  // Phones show one column or an overview (BRD-088).
  const narrow = new MediaQuery(NARROW, false);
  const zoomed = $derived(new BoardZoom(board.id));
  const zoom = $derived(narrow.current ? zoomed.value : null);

  // The backlog panel (BRD-089): open or closed is kept in this browser per board.
  const backlogKey = (id: number) => `trackflow.board-backlog.${String(id)}`;
  let backlogOpen = $state(untrack(() => readStored(backlogKey(board.id)) === 'open'));
  const backlog = $derived(new BacklogState(board.id));
  $effect(() => {
    if (backlogOpen) void backlog.show(query);
  });
  // The backlog follows the board's settings: a switch of its search, a new one.
  let backlogSearch: number | null | undefined;
  $effect(() => {
    const now = board.backlog;
    if (backlogSearch !== undefined && now !== backlogSearch) {
      untrack(() => void backlog.reload());
    }
    backlogSearch = now;
  });
  $effect(() => {
    const shown = follower;
    shown.backlogOrder = (issue, after) => {
      backlog.moved(issue, after);
    };
  });

  function toggleBacklog(): void {
    backlogOpen = !backlogOpen;
    writeStored(backlogKey(board.id), backlogOpen ? 'open' : null);
  }

  /** The person may put this backlog issue on the shown sprint (or the board). */
  function canAdd(issue: Pick<Issue, 'can'>): boolean {
    return model.sprint !== null && issue.can.update;
  }

  function backlogRow(issue: number): Issue | undefined {
    return backlog.list?.items.find((one) => one.id === issue);
  }

  // Adding to a query board shows the field changes first (BRD-028, owner decision 2).
  let queryOpen = $state(false);
  let pending = $state<{ issue: number; to: DropTarget | null; id: string; lines: ChangeLine[] }>({
    issue: 0,
    to: null,
    id: '',
    lines: [],
  });

  async function addFromBacklog(issue: number, to: DropTarget | null): Promise<void> {
    const row = backlogRow(issue);
    if (board.mode === 'query') {
      const placed = await model.preview(issue, to);
      if (!placed) return;
      pending = {
        issue,
        to,
        id: row?.readable_id ?? placed.issue?.readable_id ?? '',
        lines: changeLines(placed, row ?? null),
      };
      queryOpen = true;
      return;
    }
    await commitAdd(issue, to);
  }

  async function commitAdd(issue: number, to: DropTarget | null): Promise<void> {
    const placed = await model.add(issue, to);
    if (!placed) return;
    backlog.take(issue);
    toasts.show({
      tone: 'success',
      message: board.sprints_on ? m.addedToSprint() : m.addedToBoard(),
    });
  }

  async function reorderBacklog(issue: number, index: number): Promise<void> {
    try {
      await backlog.order(issue, index);
    } catch (error) {
      showError(new Error(m.orderFailed(), { cause: error }));
    }
  }

  /** A card dropped on the backlog leaves the sprint or the board (BRD-074). */
  async function sendBack(issue: number): Promise<void> {
    if (!model.canRemove(issue)) return;
    await model.remove(issue);
    void backlog.reload();
  }

  const drag = $derived(
    new BoardDrag({
      canDrag: (issue, fromBacklog) => {
        if (!fromBacklog) return model.canMove(issue);
        const row = backlogRow(issue);
        if (!row) return false;
        return backlog.meta?.search?.may_order === true || canAdd(row);
      },
      drop: (issue, target) => void model.move(issue, target),
      toBacklog: (issue, index, fromBacklog) => {
        if (!fromBacklog) void sendBack(issue);
        else if (backlog.meta?.search?.may_order) void reorderBacklog(issue, index);
      },
      fromBacklog: (issue, target) => {
        const row = backlogRow(issue);
        if (row && canAdd(row)) void addFromBacklog(issue, target);
      },
      active: (on) => {
        model.setDragging(on);
      },
      paged: () => zoom === 'column',
    }),
  );
</script>

{#snippet backlogPanel()}
  <BacklogPanel
    {board}
    {backlog}
    {drag}
    {canAdd}
    {live}
    onadd={(issue: Issue) => addFromBacklog(issue.id, null)}
  />
{/snippet}

<svelte:head><title>{board.name} · trackflow</title></svelte:head>

<div class="page">
  {#if model.gone}
    <NotFoundState home={boardHref.list()} />
  {:else}
    <header class="header">
      <h1><BoardSwitcher {board} /></h1>
    </header>
  {/if}

  {#if model.gone}
    <!-- The not-found state above. -->
  {:else if model.view}
    <BoardToolbar
      {board}
      columns={model.view.columns}
      estimate={model.view.totals.estimate}
      {size}
      {zoom}
      onzoom={(one: Zoom) => {
        zoomed.set(one);
      }}
      onsize={(card_size: CardSize) => model.setProfile({ card_size })}
      onsettings={() => {
        settingsOpen = true;
      }}
      create={createItems}
      sorted={model.sorted}
      backlog={backlogOpen}
      onbacklog={toggleBacklog}
    >
      {#snippet sprint()}
        {#if board.sprints_on}
          <SprintMenu
            {board}
            {sprints}
            shown={shownSprint}
            q={query}
            onchoose={chooseSprint}
            onedit={(one: Sprint) => {
              openSprint(one);
            }}
            oncreate={() => {
              openSprint(null);
            }}
          />
          {#if shownSprint && !shownSprint.internal}
            <SprintSchedule
              sprint={shownSprint}
              edit={board.rights.edit}
              {now}
              onedit={() => {
                openSprint(shownSprint);
              }}
              onstart={() => act(shownSprint, 'start')}
              onfinish={() => {
                finishOpen = true;
              }}
            />
          {/if}
        {/if}
      {/snippet}
      {#snippet filter()}
        <QueryBar
          q={query}
          label={m.headerFilter()}
          problems={model.view?.notes ?? []}
          onsearch={search}
        />
      {/snippet}
    </BoardToolbar>
    <HealthNotice
      {board}
      onopen={board.rights.edit ? openSettingsOn : undefined}
      skip={noColumns ? ['no_column_field', 'no_columns'] : []}
    />
    <div class="body">
      {#if backlogOpen && !narrow.current}
        <aside class="side" aria-label={m.backlog()}>{@render backlogPanel()}</aside>
      {/if}
      <div class="main">
        {#if noColumns}
          <EmptyState
            icon={Columns3}
            level={2}
            title={m.noColumnField()}
            description={board.rights.edit ? m.noColumnFieldText() : m.noColumnsReader()}
          >
            {#snippet actions()}
              {#if board.rights.edit}
                <Button
                  variant="primary"
                  onclick={() => {
                    openSettingsOn('columns');
                  }}>{m.openSettings()}</Button
                >
              {/if}
            {/snippet}
          </EmptyState>
        {:else}
          <BoardGrid {board} {model} {zoom} {drag} />
        {/if}
      </div>
    </div>
    <BoardFooter
      {board}
      sprint={shownSprint}
      onrestore={() => shownSprint && act(shownSprint, 'restore')}
      ondelete={() => {
        openSprint(shownSprint, true);
      }}
      onnewlane={model.canCreateLane ? () => (laneOpen = true) : undefined}
    />
  {:else if model.error}
    <ErrorState
      error={model.error}
      message={m.loadFailed()}
      level={2}
      onretry={() => model.load()}
    />
  {:else}
    <div class="loading"><Spinner label={common.loading()} /></div>
  {/if}
</div>

{#if board.rights.edit && board.sprints_on}
  <SprintDialog
    bind:open={sprintOpen}
    {board}
    sprint={editing}
    {deleting}
    carryFrom={editing ? null : carryFrom}
    onsaved={(saved: Sprint, created: boolean) => {
      if (created) chooseSprint(saved);
      else void model.load();
    }}
    ondeleted={(gone: Sprint) => {
      if (gone.id === shownSprint?.id && chosen !== null) chooseSprint(null);
      else void model.load();
    }}
  />
  {#if shownSprint && !shownSprint.internal}
    <FinishDialog
      bind:open={finishOpen}
      {board}
      sprint={shownSprint}
      {sprints}
      onfinished={() => model.load()}
    />
  {/if}
{/if}

{#if board.rights.edit || settingsOpen}
  <SettingsPanel
    bind:open={settingsOpen}
    bind:tab={settingsTab}
    {board}
    {sprints}
    onchanged={() => model.load()}
  />
{/if}

{#if model.canCreateLane}
  <NewLaneDialog bind:open={laneOpen} {model} />
{/if}

<IssuePopup
  target={model.popup}
  onclose={() => {
    model.popup = null;
  }}
/>

{#if narrow.current}
  <Dialog
    open={backlogOpen}
    title={m.backlog()}
    hideTitle
    side="start"
    onclose={() => {
      if (backlogOpen) toggleBacklog();
    }}
  >
    {@render backlogPanel()}
  </Dialog>
{/if}

<QueryModeDialog
  bind:open={queryOpen}
  id={pending.id}
  lines={pending.lines}
  onconfirm={() => commitAdd(pending.issue, pending.to)}
/>

<style>
  .page {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 100%;
  }

  .header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-4);
    min-width: 0;
    padding: var(--space-5) var(--space-6) var(--space-3);
  }

  h1 {
    min-width: 0;
    font-size: var(--text-xl);
  }

  .body {
    display: flex;
    flex: 1;
    align-items: flex-start;
    min-width: 0;
  }

  .main {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .side {
    position: sticky;
    top: var(--board-toolbar, 6rem);
    display: flex;
    flex: 0 0 20rem;
    flex-direction: column;
    max-height: calc(100dvh - var(--board-toolbar, 6rem));
    padding: var(--space-4);
    border-right: 1px solid var(--color-border);
  }

  .side > :global(.backlog) {
    flex: 1;
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: var(--space-12);
  }

  @media (width < 48rem) {
    .header {
      padding: var(--space-4) var(--space-4) var(--space-2);
    }
  }
</style>
