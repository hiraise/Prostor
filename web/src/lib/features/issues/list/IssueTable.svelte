<script lang="ts">
  // The issue list as a table on wide screens (QRY-075, QRY-076): a star toggle, then the chosen
  // columns, and the row's actions at the end (AUT-066). A header people may sort by is a button:
  // the first click sorts ascending, the next descending; the header of the order in use says so
  // (`aria-sort`).
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';

  import type { Issue, SortUsed } from '#lib/api/schema.gen.ts';

  import { askCloning } from '../clone.svelte.ts';
  import Cell from './Cell.svelte';
  import type { ShownColumn } from './columns.ts';
  import * as m from './messages.ts';
  import RowMenu from './RowMenu.svelte';
  import RowStar from './RowStar.svelte';
  import { sortedBy } from './sort.ts';

  interface Props {
    items: readonly Issue[];
    columns: readonly ShownColumn[];
    sort: readonly SortUsed[];
    busy?: boolean;
    onsort: (column: ShownColumn) => void;
  }

  let { items, columns, sort, busy = false, onsort }: Props = $props();

  function ariaSort(direction: 'asc' | 'desc' | null): 'ascending' | 'descending' | undefined {
    if (direction === null) return undefined;
    return direction === 'asc' ? 'ascending' : 'descending';
  }
</script>

<div class="frame">
  <table aria-label={m.rows()} aria-busy={busy}>
    <thead>
      <tr>
        <th scope="col" class="star"><span class="sr-only">{m.starColumn()}</span></th>
        {#each columns as shown (shown.key)}
          {@const direction = sortedBy(shown.column, sort)}
          <th scope="col" aria-sort={ariaSort(direction)} class={shown.text}>
            {#if shown.sortName !== null}
              <button
                type="button"
                class="sort"
                onclick={() => {
                  onsort(shown);
                }}
              >
                {shown.name}
                {#if direction === 'asc'}
                  <ArrowUp size={14} aria-hidden="true" />
                {:else if direction === 'desc'}
                  <ArrowDown size={14} aria-hidden="true" />
                {/if}
              </button>
            {:else}
              {shown.name}
            {/if}
          </th>
        {/each}
        <th scope="col" class="actions"><span class="sr-only">{m.actionsColumn()}</span></th>
      </tr>
    </thead>
    <tbody>
      {#each items as issue (issue.id)}
        <tr
          class={{ resolved: issue.resolved_at !== null }}
          onpointerenter={() => askCloning(issue.project.id)}
          onfocusin={() => askCloning(issue.project.id)}
        >
          <td class="star"><RowStar {issue} /></td>
          {#each columns as shown (shown.key)}
            <td class={shown.text}><Cell {issue} column={shown.column} /></td>
          {/each}
          <td class="actions"><RowMenu {issue} /></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .frame {
    overflow-x: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-muted);
    font-weight: var(--weight-medium);
    text-align: start;
    white-space: nowrap;
  }

  td {
    padding: var(--space-1) var(--space-3);
    vertical-align: middle;
  }

  tbody tr + tr td {
    border-top: 1px solid var(--color-border);
  }

  .star {
    width: 1%;
    padding-inline: var(--space-1);
  }

  /* The row's actions show with the row's hover or focus on a mouse (AUT-066); keyboards and
     touch reach the button the same way, it is always in the tab order. */
  .actions {
    width: 1%;
    padding-inline: var(--space-1);
    text-align: end;
    white-space: nowrap;
  }

  @media (pointer: fine) {
    .actions :global(button) {
      opacity: 0;
    }

    tbody tr:hover .actions :global(button),
    tbody tr:focus-within .actions :global(button) {
      opacity: 1;
    }
  }

  .id {
    width: 1%;
    white-space: nowrap;
  }

  .summary {
    min-width: 16rem;
  }

  .sort {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 2rem;
    margin-inline: calc(-1 * var(--space-1));
    padding: 0 var(--space-1);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .sort:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }
</style>
