<script lang="ts">
  // The issue list as a table on wide screens (QRY-075, QRY-076): a selection checkbox, a star
  // toggle, then the chosen columns, and the row's actions at the end (AUT-066). A header people
  // may sort by is a button: the first click sorts ascending, the next descending; the header of
  // the order in use says so (`aria-sort`). The rows are a keyboard grid (step 2.2 §6, QRY-085):
  // roving tabindex — the focused row is the one tab stop — and the shared row keys.
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';

  import type { Issue, SortUsed } from '#lib/api/schema.gen.ts';

  import { askCloning } from '../clone.svelte.ts';
  import Cell from './Cell.svelte';
  import type { ShownColumn } from './columns.ts';
  import type { RowKeydown } from './keyboard.ts';
  import type { ListFocus, ListSelection } from './list-state.svelte.ts';
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
    /** The roving focus of the list's rows (§6). */
    focus: ListFocus;
    /** The shared keys of a row (§6). */
    onrowkeydown: RowKeydown;
    /** The list's selection (§8.1): the checkboxes read and drive it. */
    selection: ListSelection;
    /** Runs after a checkbox changed the selection: the count is announced (§8.1). */
    onselected: () => void;
  }

  let {
    items,
    columns,
    sort,
    busy = false,
    onsort,
    focus,
    onrowkeydown,
    selection,
    onselected,
  }: Props = $props();

  const pageIds = $derived(items.map((issue) => issue.id));
  const pageChosen = $derived(pageIds.every((id) => selection.has(id)));
  const pageMixed = $derived(!pageChosen && pageIds.some((id) => selection.has(id)));
  let headerCheck: HTMLInputElement | undefined = $state();

  $effect(() => {
    // The half-chosen page shows an indeterminate header (§8.1); the property is not an attribute.
    if (headerCheck) headerCheck.indeterminate = pageMixed;
  });

  function ariaSort(direction: 'asc' | 'desc' | null): 'ascending' | 'descending' | undefined {
    if (direction === null) return undefined;
    return direction === 'asc' ? 'ascending' : 'descending';
  }
</script>

<div class="frame">
  <table role="grid" aria-label={m.rows()} aria-busy={busy}>
    <thead>
      <tr>
        <th scope="col" class="check">
          <!-- The page's issues all chosen or let go (§8.1); the half-chosen page is indeterminate.
               The browser toggles the input, the model stays the one writer of the rest — Svelte
               re-syncs the property when the model changes without this click (§8.1's offer). -->
          <input
            bind:this={headerCheck}
            type="checkbox"
            checked={pageChosen}
            aria-label={m.selectPage()}
            onclick={() => {
              selection.selectPage(!pageChosen, pageIds);
              onselected();
            }}
          />
        </th>
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
      {#each items as issue, index (issue.id)}
        {@const tab =
          focus.current === null ? (index === 0 ? 0 : -1) : focus.current === issue.id ? 0 : -1}
        <tr
          data-row={issue.id}
          tabindex={tab}
          class={{ resolved: issue.resolved_at !== null }}
          onpointerenter={() => askCloning(issue.project.id)}
          onfocusin={() => {
            askCloning(issue.project.id);
            // A click or Tab into the row's cells makes it the focused row (§6).
            focus.current = issue.id;
          }}
          onkeydown={(event: KeyboardEvent) => {
            onrowkeydown(event, issue);
          }}
        >
          <td class="check">
            <input
              type="checkbox"
              checked={selection.has(issue.id)}
              aria-label={m.selectRow({ id: issue.readable_id ?? String(issue.id) })}
              onclick={(event: MouseEvent) => {
                selection.click(issue.id, pageIds, event.shiftKey);
                onselected();
              }}
            />
          </td>
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

  .star,
  .check {
    width: 1%;
    padding-inline: var(--space-1);
  }

  .check input {
    /* A touch target that reaches past its box on phones (D-019). */
    margin: 0;
    accent-color: var(--color-ink);
    cursor: pointer;
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
