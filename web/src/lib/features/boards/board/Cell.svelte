<script lang="ts">
  // A cell: the cards of one column (in one lane) in the shared order; a truncated cell ends with
  // «Ещё N», which shows the next page (BRD-057). Its «+» opens the new-issue dialog on the cell's
  // draft, so the card lands in the cell (BRD-077).
  import Plus from '@lucide/svelte/icons/plus';

  import type { Board, BoardColumn } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';

  import type { CardSize } from '../boards.ts';
  import * as m from '../messages.ts';
  import type { BoardState } from './board-state.svelte.ts';
  import Card from './Card.svelte';

  interface Props {
    board: Board;
    column: BoardColumn;
    model: BoardState;
    size: CardSize;
    /** The lane key on a board with lanes. */
    lane?: string | null;
  }

  let { board, column, model, size, lane = null }: Props = $props();

  const cell = $derived(model.cell(column.id, lane));
  const hidden = $derived(model.hidden(column.id, lane));
  const label = $derived(
    lane === null
      ? m.cellCards({ name: column.value.name })
      : m.laneCellCards({ column: column.value.name, lane: model.laneLabel(lane).name }),
  );
</script>

<div class="cell" data-drop-column={column.id} data-drop-lane={lane ?? undefined}>
  {#if cell && cell.ids.length > 0}
    <ul class="cards" aria-label={label}>
      {#each cell.ids as id (id)}
        {@const issue = model.issues.get(id)}
        {#if issue}
          <li data-card={id}><Card {issue} {board} {model} {size} /></li>
        {/if}
      {/each}
    </ul>
  {:else if lane === null}
    <!-- Lanes make many empty cells: there they stay blank. -->
    <p class="empty">{m.emptyCell()}</p>
  {/if}
  {#if hidden > 0}
    <Button
      size="sm"
      variant="ghost"
      loading={cell?.loading}
      onclick={() => model.more(column.id, lane)}
    >
      {m.moreCards({ count: hidden })}
    </Button>
  {/if}
  {#if model.canCreate}
    <span class="add">
      <IconButton
        icon={Plus}
        size="sm"
        label={lane === null
          ? m.addInCell({ name: column.value.name })
          : m.addInCell({ name: `${column.value.name} · ${model.laneLabel(lane).name}` })}
        onclick={() => {
          void model.createInCell(column.id, lane);
        }}
      />
    </span>
  {/if}
</div>

<style>
  .cell {
    display: flex;
    flex-direction: column;
    gap: var(--space-1_5);
    min-height: 4rem;
    height: 100%;
    padding: var(--space-1_5);
  }

  .cards {
    display: flex;
    user-select: none;
    -webkit-touch-callout: none;
    flex-direction: column;
    gap: var(--space-1_5);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  @media (pointer: coarse) {
    .cell,
    .cards {
      gap: var(--space-2);
    }

    .cell {
      padding: var(--space-2);
    }
  }

  .add {
    display: flex;
    justify-content: center;
    opacity: 0.6;
  }

  .cell:hover .add,
  .add:focus-within {
    opacity: 1;
  }

  .empty {
    padding: var(--space-3) var(--space-2);
    color: var(--color-text-faint);
    font-size: var(--text-sm);
    text-align: center;
  }
</style>
