<script lang="ts">
  // The board's toolbar (BRD-082): the sprint menu with the schedule (BRD-083), progress, the zoom
  // on phones (BRD-088), the card size, the settings and the header filter (BRD-081) with «Порядок
  // по запросу» while it sorts (BRD-070), and the backlog toggle (BRD-089). Without edit
  // rights the gear stays in place, says why in its tooltip and does nothing. «Новая» opens the
  // new-issue dialog (BRD-076). On a phone the filter opens on demand; an active one stays marked.
  import Plus from '@lucide/svelte/icons/plus';
  import ArrowDownUp from '@lucide/svelte/icons/arrow-down-up';
  import PanelLeft from '@lucide/svelte/icons/panel-left';
  import Search from '@lucide/svelte/icons/search';
  import Settings from '@lucide/svelte/icons/settings';
  import RectangleHorizontal from '@lucide/svelte/icons/rectangle-horizontal';
  import type { Snippet } from 'svelte';

  import type { Board, ColumnCount } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu from '#lib/ui/Menu.svelte';

  import { CARD_SIZES, estimateText, type CardSize } from '../boards.ts';
  import * as m from '../messages.ts';
  import Progress from './Progress.svelte';
  import { ZOOMS, type Zoom } from './zoom.svelte.ts';

  interface Props {
    board: Board;
    columns: ColumnCount[];
    size: CardSize;
    onsize: (size: CardSize) => unknown;
    onsettings: () => unknown;
    /** The zoom on a phone; `null` on wider screens. */
    zoom?: Zoom | null;
    onzoom?: (zoom: Zoom) => unknown;
    /** The sprint menu and the schedule of a sprint board. */
    sprint?: Snippet;
    /** The header filter. */
    filter?: Snippet;
    filterActive?: boolean;
    /** The header filter sorts the cells. */
    sorted?: boolean;
    /** The estimation of every card (BRD-062); `null` without an estimation field. */
    estimate?: number | null;
    /** The backlog panel is shown. */
    backlog?: boolean;
    onbacklog?: () => unknown;
    /** «Новая» opens the new-issue dialog; `undefined` hides the button. */
    oncreate?: () => unknown;
  }

  let {
    board,
    columns,
    size,
    onsize,
    onsettings,
    zoom = null,
    onzoom,
    sprint,
    filter,
    filterActive = false,
    sorted = false,
    estimate = null,
    backlog = false,
    onbacklog,
    oncreate,
  }: Props = $props();

  let filterOpen = $state(false);
  const items = $derived(
    CARD_SIZES.map((one) => ({
      type: 'radio' as const,
      label: m.cardSizes[one]?.() ?? '',
      checked: one === size,
      onSelect: () => void onsize(one),
    })),
  );
</script>

<div
  class={['toolbar', { mobile: zoom !== null, 'filter-open': filterOpen }]}
  role="toolbar"
  aria-label={m.toolbar()}
>
  <div class="main-row">
    <div class="controls">
      {#if onbacklog}
        <div class="backlog">
          <Button
            size="sm"
            variant={backlog ? 'secondary' : 'ghost'}
            icon={PanelLeft}
            aria-pressed={backlog}
            onclick={() => onbacklog()}><span class="backlog-label">{m.backlog()}</span></Button
          >
        </div>
      {/if}
      {#if sprint}<div class="sprint">{@render sprint()}</div>{/if}
      {#if estimate !== null}
        <span class="estimate">{m.estimateSum({ value: estimateText(board, estimate) })}</span>
      {/if}
      <div class="end">
        {#if oncreate}
          <div class="create">
            <Button size="sm" variant="secondary" icon={Plus} onclick={() => oncreate()}>
              {m.newMenu()}
            </Button>
          </div>
        {/if}
        {#if zoom !== null}
          <div class="zoom" role="group" aria-label={m.zoom()}>
            {#each ZOOMS as one (one)}
              <Button
                size="sm"
                variant={one === zoom ? 'secondary' : 'ghost'}
                aria-pressed={one === zoom}
                onclick={() => onzoom?.(one)}>{m.zooms[one]()}</Button
              >
            {/each}
          </div>
        {/if}
        {#if zoom !== 'all'}
          <Menu label={m.cardSize()} {items} placement={{ side: 'bottom', align: 'end' }}>
            {#snippet trigger(props)}
              <Button size="sm" variant="ghost" icon={RectangleHorizontal} {...props}>
                <span class="size-label">{m.cardSize()}:</span>
                {m.cardSizes[size]?.()}
              </Button>
            {/snippet}
          </Menu>
        {/if}
        {#if zoom !== null && filter}
          <IconButton
            icon={Search}
            label={m.headerFilter()}
            pressed={filterOpen || filterActive}
            aria-expanded={filterOpen}
            aria-controls="board-header-filter"
            onclick={() => {
              filterOpen = !filterOpen;
            }}
          />
        {/if}
        {#if board.rights.edit}
          <IconButton icon={Settings} label={m.settings()} onclick={() => onsettings()} />
        {:else}
          <IconButton
            icon={Settings}
            label={m.settingsLocked()}
            aria-disabled="true"
            class="locked"
          />
        {/if}
      </div>
    </div>
    <div class="progress"><Progress {board} {columns} /></div>
  </div>
  {#if filter}
    <div class="filter" id="board-header-filter">
      {@render filter()}
      {#if sorted}
        <span class="sorted"><ArrowDownUp aria-hidden="true" />{m.sortedByQuery()}</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .estimate {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .toolbar {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-canvas);
  }

  .main-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-width: 0;
  }

  /* The scrollable controls row is wider than a phone: clipped here, it must not widen the
     page's own width, or a phone Chrome zooms the whole board out when a popover opens. */
  .mobile {
    overflow: hidden;
  }

  .controls {
    display: contents;
  }

  .backlog {
    order: 0;
  }

  .sprint {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-width: 0;
    order: 1;
  }

  .progress {
    display: flex;
    flex: 1 1 8rem;
    max-width: 16rem;
    min-width: 4rem;
    order: 2;
  }

  .progress > :global(.progress) {
    width: 100%;
  }

  .estimate {
    order: 3;
  }

  .end {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-1);
    margin-inline-start: auto;
    order: 4;
  }

  .filter {
    display: flex;
    flex: 1 0 100%;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .filter > :global(:first-child) {
    flex: 1 1 20rem;
    min-width: 0;
  }

  .sorted {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .sorted :global(svg) {
    width: var(--icon-sm);
    height: var(--icon-sm);
  }

  .zoom {
    display: flex;
    flex: none;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }

  .zoom :global(.button) {
    min-width: 2.75rem;
  }

  .toolbar :global(.locked) {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .mobile .main-row {
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .mobile .controls {
    display: flex;
    flex: 1 0 100%;
    align-items: center;
    gap: var(--space-1_5);
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .mobile .sprint {
    flex: none;
    flex-wrap: nowrap;
  }

  .mobile .backlog-label {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .mobile .backlog :global(.button) {
    min-width: var(--control-sm);
  }

  .mobile .create {
    margin-inline-end: var(--space-2);
  }

  .mobile .size-label {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .mobile .progress {
    flex: 1 0 100%;
    max-width: none;
    min-width: 0;
    padding: 0 var(--space-1) var(--space-1);
  }

  .mobile .progress > :global(.progress) {
    height: var(--space-1);
  }

  .mobile:not(.filter-open) .filter {
    display: none;
  }

  .mobile .filter {
    padding-top: var(--space-1);
  }

  .mobile .end :global(.icon-button) {
    flex: 0 0 var(--control-sm);
    width: var(--control-sm);
    height: var(--control-sm);
  }

  .mobile .end :global(.icon-button svg) {
    width: var(--icon-lg);
    height: var(--icon-lg);
  }

  @media (width <= 64rem) {
    .size-label {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }
  }
</style>
