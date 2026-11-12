<script lang="ts">
  // The board's toolbar (BRD-082): the sprint menu with the schedule (BRD-083), progress, the zoom
  // on phones (BRD-088), the card size, the settings and the header filter (BRD-081) with «Порядок
  // по запросу» while it sorts (BRD-070), and the backlog toggle (BRD-089). Without edit
  // rights the gear stays in place, says why in its tooltip and does nothing. «Новая» makes a
  // card, a lane, a sprint or a board (BRD-076).
  import Plus from '@lucide/svelte/icons/plus';
  import ArrowDownUp from '@lucide/svelte/icons/arrow-down-up';
  import PanelLeft from '@lucide/svelte/icons/panel-left';
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
    /** The header filter sorts the cells. */
    sorted?: boolean;
    /** The estimation of every card (BRD-062); `null` without an estimation field. */
    estimate?: number | null;
    /** The backlog panel is shown. */
    backlog?: boolean;
    onbacklog?: () => unknown;
    /** The entries of «Новая» the person may use. */
    create?: { label: string; onSelect: () => unknown }[];
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
    sorted = false,
    estimate = null,
    backlog = false,
    onbacklog,
    create = [],
  }: Props = $props();

  const items = $derived(
    CARD_SIZES.map((one) => ({
      type: 'radio' as const,
      label: m.cardSizes[one]?.() ?? '',
      checked: one === size,
      onSelect: () => void onsize(one),
    })),
  );
</script>

<div class="toolbar" role="toolbar" aria-label={m.toolbar()}>
  {#if onbacklog}
    <Button
      size="sm"
      variant={backlog ? 'secondary' : 'ghost'}
      icon={PanelLeft}
      aria-pressed={backlog}
      onclick={() => onbacklog()}>{m.backlog()}</Button
    >
  {/if}
  {#if sprint}<div class="sprint">{@render sprint()}</div>{/if}
  <Progress {board} {columns} />
  {#if estimate !== null}
    <span class="estimate">{m.estimateSum({ value: estimateText(board, estimate) })}</span>
  {/if}
  <div class="end">
    {#if create.length > 0}
      <Menu
        label={m.newMenu()}
        items={create.map((one) => ({
          type: 'action' as const,
          label: one.label,
          onSelect: () => void one.onSelect(),
        }))}
        placement={{ side: 'bottom', align: 'end' }}
      >
        {#snippet trigger(props)}
          <Button size="sm" variant="secondary" icon={Plus} {...props}>{m.newMenu()}</Button>
        {/snippet}
      </Menu>
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
    {#if board.rights.edit}
      <IconButton icon={Settings} label={m.settings()} onclick={() => onsettings()} />
    {:else}
      <IconButton icon={Settings} label={m.settingsLocked()} aria-disabled="true" class="locked" />
    {/if}
  </div>
  {#if filter}
    <div class="filter">
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
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-2) var(--space-6);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-canvas);
  }

  .end {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin-inline-start: auto;
  }

  .sprint {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-width: 0;
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

  @media (width < 48rem) {
    .toolbar {
      padding: var(--space-2) var(--space-4);
    }

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
