<script lang="ts">
  // A column's header (BRD-046, BRD-047, §3.7): the value, the count, the limits, a warning in
  // words and with an icon when the count breaks a limit, collapse for this person, and
  // «Открыть списком» with the board projects and the column value as the query.
  import ChevronsLeftRight from '@lucide/svelte/icons/chevrons-left-right';
  import ChevronsRightLeft from '@lucide/svelte/icons/chevrons-right-left';
  import List from '@lucide/svelte/icons/list';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import type { Board, BoardColumn, ColumnCount, Sprint } from '#lib/api/schema.gen.ts';
  import { listHref } from '#lib/features/issues/index.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import { columnQuery, estimateText } from '../boards.ts';
  import * as m from '../messages.ts';

  interface Props {
    board: Board;
    column: BoardColumn;
    /** The shown sprint: «Открыть списком» keeps to it. */
    sprint?: Sprint | null;
    count: ColumnCount;
    collapsed: boolean;
    /** Whether the person can collapse it (not in the one-column zoom of a phone). */
    collapsible?: boolean;
    ontoggle: () => unknown;
  }

  let {
    board,
    column,
    sprint = null,
    count,
    collapsed,
    collapsible = true,
    ontoggle,
  }: Props = $props();

  const name = $derived(column.value.name);
  const limits = $derived(
    [
      column.wip_min !== null ? m.wipMin({ count: column.wip_min }) : null,
      column.wip_max !== null ? m.wipMax({ count: column.wip_max }) : null,
    ]
      .filter(Boolean)
      .join(', '),
  );
  const warning = $derived(
    count.wip === 'over' && column.wip_max !== null
      ? m.wipOver({ count: column.wip_max })
      : count.wip === 'under' && column.wip_min !== null
        ? m.wipUnder({ count: column.wip_min })
        : null,
  );
  const listLink = $derived(
    listHref({ q: columnQuery(board, column, sprint), context: { kind: 'all' } }),
  );
</script>

<div class={['header', { collapsed, broken: warning !== null }]}>
  <div class="line">
    <h2 class="name">{name}</h2>
    <span class="count" aria-hidden="true">{count.count}</span>
    <span class="sr-only">{m.cardCount({ count: count.count })}</span>
    {#if count.estimate !== null}
      <span class="count estimate">{estimateText(board, count.estimate)}</span>
    {/if}
  </div>
  {#if !collapsed && (limits || warning)}
    <p class="limits">
      {#if warning}
        <span class="warning"><TriangleAlert size={14} aria-hidden="true" />{warning}</span>
      {:else}
        {limits}
      {/if}
    </p>
  {:else if collapsed && warning}
    <span class="warning" role="img" aria-label={warning}>
      <TriangleAlert size={14} aria-hidden="true" />
    </span>
  {/if}
  <div class="actions">
    {#if collapsible}
      <IconButton
        icon={collapsed ? ChevronsLeftRight : ChevronsRightLeft}
        size="sm"
        label={collapsed ? m.expandColumn({ name }) : m.collapseColumn({ name })}
        aria-expanded={!collapsed}
        onclick={() => ontoggle()}
      />
    {/if}
    {#if !collapsed}
      <!-- listHref resolves the address. -->
      <!-- eslint-disable svelte/no-navigation-without-resolve -->
      <a
        href={listLink}
        class="list"
        aria-label={m.openAsList({ name })}
        {@attach tooltip(m.openAsList({ name }))}
      >
        <List aria-hidden="true" />
      </a>
      <!-- eslint-enable svelte/no-navigation-without-resolve -->
    {/if}
  </div>
</div>

<style>
  .header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    padding: var(--space-1_5) var(--space-2) var(--space-1_5) var(--space-3);
    border-top: 1px solid var(--color-border);
  }

  /* An absolutely positioned sr-only span makes a phone Chrome zoom the board out when a
     dialog opens; a zero-size inline block hides the words just as well. */
  .line :global(.sr-only) {
    position: static;
    display: inline-block;
    width: 0;
    height: 0;
    overflow: hidden;
    clip-path: none;
    white-space: normal;
  }

  .header.broken {
    border-top-color: var(--color-danger);
  }

  .line {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    flex-shrink: 0;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .limits {
    order: 3;
    flex-basis: 100%;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .warning {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-danger);
    font-weight: var(--weight-medium);
  }

  .actions {
    display: flex;
    gap: var(--space-1);
  }

  .list {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-sm);
    height: var(--control-sm);
    border-radius: var(--radius-md);
    color: var(--color-text-muted);
  }

  .list:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .list :global(svg) {
    width: 1rem;
    height: 1rem;
  }

  /* A collapsed column is a narrow strip: the button on top, the name down the side. */
  .collapsed {
    flex-direction: column;
    flex-wrap: nowrap;
    padding: var(--space-2) var(--space-1);
  }

  .collapsed .line {
    flex-direction: column;
    align-items: center;
    order: 2;
  }

  .collapsed .name {
    writing-mode: vertical-rl;
    max-height: 16rem;
  }

  .collapsed .actions {
    order: 1;
  }
</style>
