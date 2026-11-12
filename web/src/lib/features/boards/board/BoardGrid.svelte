<script lang="ts">
  // The grid of the board (BRD-068): the columns side by side with their headers and cells,
  // scrolling sideways when they do not fit. With lanes (BRD-049…BRD-057) the column headers stay
  // on top and each lane is a row of cells under its header; a person collapses lanes one by one
  // or all at once; the next page of lanes loads when the end of the grid comes into view.
  //
  // On phones (BRD-088, D-044) it has two zooms. «Колонка»: one column at a time, full
  // width, swiped with snapping, a strip of names and counts above to jump; a card held at the
  // screen's edge while dragged goes to the next column. «Все»: every column narrow with the
  // smallest cards; tapping a column's header hides or shows its cards (for this visit only).
  import { SvelteSet } from 'svelte/reactivity';

  import type { Board, BoardColumn, ColumnCount } from '#lib/api/schema.gen.ts';

  import { cardSize, type CardSize } from '../boards.ts';
  import type { BoardDrag } from '../drag/drag.svelte.ts';
  import * as m from '../messages.ts';
  import type { BoardState } from './board-state.svelte.ts';
  import Button from '#lib/ui/Button.svelte';

  import LaneHeader from '../lanes/LaneHeader.svelte';
  import Cell from './Cell.svelte';
  import ColumnHeader from './ColumnHeader.svelte';
  import type { Zoom } from './zoom.svelte.ts';

  interface Props {
    board: Board;
    model: BoardState;
    /** The zoom on a phone; `null` on wider screens. */
    zoom?: Zoom | null;
    /** Drags cards here and to and from the backlog (the page makes it). */
    drag: BoardDrag;
  }

  let { board, model, zoom = null, drag }: Props = $props();

  let grid = $state<HTMLElement | null>(null);
  /** The column shown in the one-column zoom. */
  let current = $state(0);
  /** Columns whose cards the overview hides. */
  const folded = new SvelteSet<number>();

  const size = $derived<CardSize>(zoom === 'all' ? 0 : cardSize(model.profile?.card_size ?? 1));
  const collapsed = $derived(
    zoom === null ? new Set(model.profile?.collapsed_columns ?? []) : new Set<number>(),
  );
  const counts = $derived(new Map(model.view?.columns.map((one) => [one.column, one]) ?? []));
  const shown = $derived(board.columns.filter((column) => counts.has(column.id)));
  const lanesCollapsed = $derived(new Set(model.profile?.collapsed_lanes ?? []));
  const lanesLeft = $derived(Math.max(0, (model.view?.lanes_total ?? 0) - model.lanes.length));

  function columnsOf(node: HTMLElement): HTMLElement[] {
    return [...node.querySelectorAll<HTMLElement>(':scope > .column, :scope > .heads > .column')];
  }

  /** Loads the next page of lanes when the end of the grid comes near (BRD-057). */
  function sentinel(node: HTMLElement): () => void {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) void model.moreLanes();
      },
      { root: grid, rootMargin: '400px' },
    );
    observer.observe(node);
    return () => {
      observer.disconnect();
    };
  }

  /** The column nearest the start of the scrolled grid. */
  function follow(): void {
    if (!grid || zoom !== 'column') return;
    const start = grid.getBoundingClientRect().left;
    let best = 0;
    let distance = Infinity;
    columnsOf(grid).forEach((column, index) => {
      const away = Math.abs(column.getBoundingClientRect().left - start);
      if (away < distance) {
        best = index;
        distance = away;
      }
    });
    current = best;
  }

  function jump(index: number): void {
    const column = grid ? columnsOf(grid)[index] : undefined;
    if (!grid || !column) return;
    const left = grid.scrollLeft + column.getBoundingClientRect().left;
    grid.scrollTo({ left: left - grid.getBoundingClientRect().left, behavior: 'smooth' });
    current = index;
  }

  function fold(column: number): void {
    if (!folded.delete(column)) folded.add(column);
  }
</script>

{#if zoom === 'column'}
  <nav class="strip" aria-label={m.columnStrip()}>
    {#each shown as column, index (column.id)}
      {@const count = counts.get(column.id)}
      <button
        type="button"
        class={['tab', { current: index === current }]}
        aria-current={index === current ? 'true' : undefined}
        onclick={() => {
          jump(index);
        }}
      >
        <span class="tab-name">{column.value.name}</span>
        <span class="tab-count">{count?.count ?? 0}</span>
      </button>
    {/each}
  </nav>
{/if}

{#snippet overview(column: BoardColumn, count: ColumnCount)}
  <button
    type="button"
    class={['overview-head', { broken: count.wip !== 'within' }]}
    aria-expanded={!folded.has(column.id)}
    onclick={() => {
      fold(column.id);
    }}
  >
    <h2 class="overview-name">{column.value.name}</h2>
    <span class="overview-count">{count.count}</span>
  </button>
{/snippet}

{#if model.lanesOn}
  <div
    class={['grid', 'with-lanes', zoom && `zoom-${zoom}`]}
    role="region"
    aria-label={m.lanesLabel()}
    tabindex="-1"
    bind:this={grid}
    onscroll={follow}
    {@attach drag.root}
  >
    <div class="heads">
      {#each shown as column (column.id)}
        {@const count = counts.get(column.id)}
        {#if count}
          <div
            class={['column', `size-${String(size)}`, { collapsed: collapsed.has(column.id) }]}
            role="group"
            aria-label={column.value.name}
          >
            {#if zoom === 'all'}
              {@render overview(column, count)}
            {:else}
              <ColumnHeader
                {board}
                {column}
                sprint={model.view?.sprint ?? null}
                {count}
                collapsed={collapsed.has(column.id)}
                collapsible={zoom === null}
                ontoggle={() => model.toggleColumn(column.id)}
              />
            {/if}
          </div>
        {/if}
      {/each}
    </div>
    {#if model.lanes.length > 1}
      <div class="lane-tools">
        <Button
          size="sm"
          variant="ghost"
          onclick={() => model.collapseLanes(!model.allLanesCollapsed)}
        >
          {model.allLanesCollapsed ? m.expandAllLanes() : m.collapseAllLanes()}
        </Button>
      </div>
    {/if}
    {#each model.lanes as lane (lane.key)}
      {@const shut = lanesCollapsed.has(lane.key)}
      <section class="lane" aria-label={m.laneLabel({ name: model.laneLabel(lane.key).name })}>
        <LaneHeader {board} {model} {lane} collapsed={shut} />
        {#if !shut}
          <div class="row">
            {#each shown as column (column.id)}
              <div
                class={[
                  'column',
                  'slot',
                  `size-${String(size)}`,
                  { collapsed: collapsed.has(column.id) },
                ]}
              >
                {#if !collapsed.has(column.id) && !(zoom === 'all' && folded.has(column.id))}
                  <Cell {board} {column} {model} {size} lane={lane.key} />
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </section>
    {/each}
    {#if model.lanesNext !== null}
      <div class="more-lanes" {@attach sentinel}>
        <Button
          size="sm"
          variant="ghost"
          loading={model.lanesLoading}
          onclick={() => model.moreLanes()}
        >
          {m.moreLanes({ count: lanesLeft })}
        </Button>
      </div>
    {/if}
  </div>
{:else}
  <div
    class={['grid', zoom && `zoom-${zoom}`]}
    role="region"
    aria-label={m.columnsLabel()}
    tabindex="-1"
    bind:this={grid}
    onscroll={follow}
    {@attach drag.root}
  >
    {#each shown as column (column.id)}
      {@const count = counts.get(column.id)}
      {#if count}
        {@const hidden = zoom === 'all' && folded.has(column.id)}
        <section
          class={['column', `size-${String(size)}`, { collapsed: collapsed.has(column.id) }]}
          aria-label={column.value.name}
        >
          {#if zoom === 'all'}
            {@render overview(column, count)}
          {:else}
            <ColumnHeader
              {board}
              {column}
              sprint={model.view?.sprint ?? null}
              {count}
              collapsed={collapsed.has(column.id)}
              collapsible={zoom === null}
              ontoggle={() => model.toggleColumn(column.id)}
            />
          {/if}
          {#if !collapsed.has(column.id) && !hidden}
            <Cell {board} {column} {model} {size} />
          {/if}
        </section>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .grid {
    display: flex;
    flex: 1;
    align-items: flex-start;
    gap: var(--space-3);
    min-height: 0;
    padding: var(--space-3) var(--space-6) var(--space-6);
    overflow: auto;
    overscroll-behavior-x: contain;
  }

  .grid:focus {
    outline: none;
  }

  /* Lanes: the column headers on top, then a row of cells per lane. */
  .grid.with-lanes {
    display: block;
    container-type: inline-size;
  }

  .heads,
  .row {
    display: flex;
    gap: var(--space-3);
    width: max-content;
  }

  .heads {
    position: sticky;
    z-index: 1;
    top: 0;
    background: var(--color-surface);
  }

  .heads .column {
    border-radius: var(--radius-md) var(--radius-md) 0 0;
  }

  .row {
    align-items: stretch;
  }

  .row .slot {
    border-radius: 0 0 var(--radius-md) var(--radius-md);
  }

  .lane {
    margin-top: var(--space-2);
    padding-top: var(--space-1);
    border-top: 1px solid var(--color-border);
  }

  .lane-tools,
  .more-lanes {
    position: sticky;
    left: 0;
    width: fit-content;
    padding: var(--space-2) 0;
  }

  .grid.with-lanes.zoom-column .column {
    flex-basis: calc(100cqi - 2 * var(--space-4));
  }

  .column {
    display: flex;
    flex: 0 0 17rem;
    flex-direction: column;
    min-width: 0;
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }

  .column.size-0 {
    flex-basis: 14rem;
  }

  .column.size-2 {
    flex-basis: 19rem;
  }

  .column.size-3 {
    flex-basis: 22rem;
  }

  .column.collapsed {
    flex-basis: auto;
    width: 3rem;
  }

  @media (pointer: coarse) {
    .column.collapsed {
      width: 3.5rem;
    }
  }

  @media (width < 48rem) {
    .grid {
      padding: var(--space-2) var(--space-4) var(--space-4);
    }
  }

  /* One column at a time: full width, swiped, snapping to each column. */
  .grid.zoom-column {
    scroll-snap-type: x mandatory;
    scroll-padding-inline: var(--space-4);
  }

  .grid.zoom-column .column {
    flex-basis: 100%;
    scroll-snap-align: start;
    scroll-snap-stop: always;
  }

  /* The overview: every column narrow. */
  .grid.zoom-all {
    gap: var(--space-2);
  }

  .grid.zoom-all .column {
    flex-basis: 10.5rem;
  }

  .overview-head {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-height: 2.75rem;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-top: 3px solid var(--color-border);
    border-radius: var(--radius-md) var(--radius-md) 0 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .overview-head.broken {
    border-top-color: var(--color-danger);
  }

  .overview-head[aria-expanded='false'] {
    border-radius: var(--radius-md);
  }

  .overview-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .overview-count,
  .tab-count {
    flex-shrink: 0;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  /* The strip of column names above the one-column zoom. */
  .strip {
    display: flex;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-4) 0;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab {
    display: inline-flex;
    flex-shrink: 0;
    align-items: baseline;
    gap: var(--space-2);
    min-height: 2.75rem;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .tab.current {
    border-bottom-color: var(--color-brand);
    color: var(--color-text);
    font-weight: var(--weight-semibold);
  }

  .tab-name {
    max-width: 10rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
