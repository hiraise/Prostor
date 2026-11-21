<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // The board page (BRD-068): the header with the switcher, the sticky toolbar with the sprint
  // controls (BRD-083) and the header filter (BRD-081), the columns with their cards, the footer
  // (BRD-084) and the settings panel (BRD-086). A board created a moment ago opens with its
  // settings (`?settings`). `/boards/{id}` shows the current sprint, `/boards/{id}/sprints/{s}` a
  // chosen one (BRD-017); `?q=` is the header filter, kept across boards (BRD-070). The backlog
  // panel (BRD-089…BRD-096) sits beside the columns, a sheet on phones; cards drag between them.
  // «Новая» opens the new-issue dialog on the first column's draft (BRD-076); cards open in the
  // issue popup (BRD-079); a notice above the columns lists what is wrong with the settings
  // (BRD-023).
  import Columns3 from '@lucide/svelte/icons/columns-3';
  import { onMount, untrack } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';

  import { goto } from '$app/navigation';

  import type { Board, Issue, QueryProblem, Sprint } from '#lib/api/schema.gen.ts';
  import { QueryBar } from '#lib/features/issues/index.ts';
  import { newIssueDialog } from '#lib/features/issues/new/open.svelte.ts';
  import { queryProblems } from '#lib/features/issues/list/search.svelte.ts';
  import { problemAt, problems } from '#lib/features/issues/list/messages.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { readStored, writeStored } from '#lib/storage.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { BOARD_OVERLAY, NARROW, PHONE_LANDSCAPE } from '#lib/theme/breakpoints.ts';
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
  import { BoardState } from './board-state.svelte.ts';
  import BoardGrid from './BoardGrid.svelte';
  import BoardToolbar from './BoardToolbar.svelte';
  import HealthNotice from './HealthNotice.svelte';
  import { boardAddress, cardAddress, headerFilter } from './header-filter.svelte.ts';
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
    /** The `issue` param of the address — the card whose popup reopens (2026-11-15). */
    card?: string | null;
    /** The current time, for tests of the schedule. */
    now?: number;
  }

  let {
    board,
    openSettings = false,
    live,
    sprint = null,
    q = null,
    card = null,
    now,
  }: Props = $props();

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

  /** «Новая»: the new-issue dialog on the first column's draft, in «Прочее» with lanes. */
  function newCard(): void {
    const column = board.columns[0];
    if (!column) return;
    const lane = model.lanesOn
      ? (model.lanes.find((one) => one.key === 'other') ?? model.lanes[0])?.key
      : null;
    if (lane === undefined) return;
    void model.createInCell(column.id, lane);
  }

  const canNew = $derived(model.canCreate && !noColumns);

  // Loads once per board, sprint and filter; what the load reads and writes must not run it again.
  // A refused filter names its problems in the bar (like the list); other failures toast.
  let filterProblems = $state<QueryProblem[] | null>(null);
  $effect(() => {
    const shown = model;
    const wanted = chosen;
    const filter = query;
    untrack(() => {
      void shown.show(wanted, filter).then(() => {
        const error = shown.error;
        if (error && shown.view) {
          const refused = queryProblems(error);
          if (refused !== null) {
            filterProblems = refused;
            return;
          }
          showError(new Error(m.filterFailed(), { cause: error }));
        } else {
          filterProblems = null;
        }
      });
    });
  });

  // The new-issue form opened on a cell draft publishes in the shell, and a publish sends no
  // board event: when the dialog closes, the view loads again and the card lands in its cell.
  $effect(() => {
    if (newIssueDialog.open || !model.awaitedDialog) return;
    model.awaitedDialog = false;
    untrack(() => void model.load());
  });

  // The filter changes the address in place: no new history entry, focus and scroll stay.
  const IN_PLACE = { replace: true, reset: false } as const;

  // A filter kept from another board joins the address; the card of the address reopens.
  onMount(() => {
    if (q === null && headerFilter.value !== '') {
      // eslint-disable-next-line svelte/no-navigation-without-resolve -- boardAddress() resolves
      void goto(boardAddress(board.id, sprint, headerFilter.value), IN_PLACE);
    }
    const reopen = Number(card);
    if (card !== null && Number.isSafeInteger(reopen) && reopen > 0) model.openIssue(reopen);
  });

  // The open card joins the address in place, so a reload reopens it (2026-11-15). Only the
  // popup's own changes write: the board's navigations (a sprint or filter change) close the
  // popup with the remount and own their addresses themselves.
  let addressedCard: number | null = null;
  $effect(() => {
    const open = model.popup?.issue ?? null;
    if (open === addressedCard) return;
    addressedCard = open;
    const base = boardAddress(
      board.id,
      untrack(() => chosen),
      untrack(() => query),
    );
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- boardAddress() resolves
    void goto(cardAddress(base, open), IN_PLACE);
  });

  function search(text: string): void {
    headerFilter.set(text);
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- boardAddress() resolves
    void goto(boardAddress(board.id, chosen, headerFilter.value), IN_PLACE);
  }

  /** One problem of a refused filter, as the list names it (QRY-058). */
  function problemText(problem: QueryProblem): string {
    const text = query.slice(problem.from, problem.to).trim();
    const meaning = problems[problem.code]();
    return text === '' ? meaning : problemAt({ text, problem: meaning });
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
  // A short touch landscape keeps the phone board even when its width crosses NARROW.
  const narrow = new MediaQuery(NARROW, false);
  const landscape = new MediaQuery(PHONE_LANDSCAPE, false);
  const overlay = new MediaQuery(BOARD_OVERLAY, false);
  const phone = $derived(narrow.current || landscape.current);
  const zoomed = $derived(new BoardZoom(board.id));
  const zoom = $derived(phone ? zoomed.value : null);

  // The backlog panel (BRD-089): open or closed is kept in this browser per board.
  const backlogKey = (id: number) => `prostor.board-backlog.${String(id)}`;
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

<svelte:head><title>{board.name} · {brand()}</title></svelte:head>

<div class={['page', { phone }]}>
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
      filterActive={query.trim() !== ''}
      onzoom={(one: Zoom) => {
        zoomed.set(one);
      }}
      onsize={(card_size: CardSize) => model.setProfile({ card_size })}
      onsettings={() => {
        settingsOpen = true;
      }}
      oncreate={canNew ? newCard : undefined}
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
          problems={filterProblems ?? model.view?.notes ?? []}
          describedby={filterProblems !== null ? 'board-filter-problems' : undefined}
          onsearch={search}
        />
      {/snippet}
    </BoardToolbar>
    {#if filterProblems !== null}
      <div class="filter-problems" id="board-filter-problems">
        <Notice tone="danger">
          <ul class="problems">
            {#each filterProblems as problem, i (i)}<li>{problemText(problem)}</li>{/each}
          </ul>
        </Notice>
      </div>
    {/if}
    <HealthNotice
      {board}
      onopen={board.rights.edit ? openSettingsOn : undefined}
      skip={noColumns ? ['no_column_field', 'no_columns'] : []}
    />
    <div class="body">
      {#if backlogOpen && !overlay.current}
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

{#if overlay.current}
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
    padding: var(--space-3) var(--space-4) var(--space-2);
  }

  h1 {
    /* Grows over the free row, so the switcher's `max-width: 100%` has a definite width to
       resolve against; a genuinely long name still ellipsizes inside it. */
    flex: 1 1 auto;
    min-width: 0;
    max-width: 100%;
    font-size: var(--text-xl);
  }

  .page.phone .header {
    padding: var(--space-2) var(--space-4) var(--space-1);
  }

  .page.phone h1 {
    font-size: var(--text-lg);
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

  .filter-problems {
    padding: var(--space-2) var(--space-6) 0;
  }

  .problems {
    margin: 0;
    padding: 0;
    padding-inline-start: var(--space-5);
  }

  @media (width < 48rem) {
    .header {
      padding: var(--space-4) var(--space-4) var(--space-2);
    }

    .filter-problems {
      padding: var(--space-2) var(--space-4) 0;
    }
  }
</style>
