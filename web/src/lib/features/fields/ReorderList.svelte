<script lang="ts" generics="T">
  // A list whose order people change (FLD-022, FLD-037, FLD-058). With a mouse, each row has a grip
  // to drag it before or after another row; the drop line shows where it lands. Dragging is only a
  // shortcut: every row's menu also has "Move up" and "Move down", which work with a keyboard, a
  // screen reader and a finger, so the grip is hidden from assistive technology and on touch
  // screens.
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import type { Snippet } from 'svelte';

  import { afterDropping } from './fields.ts';

  interface Props {
    items: readonly T[];
    key: (item: T) => number;
    /** The list's accessible name. */
    label: string;
    /** Rows can be dragged. */
    sortable?: boolean;
    /** A row was dropped: put `id` right after `after` (`null`: first). */
    onmove?: (id: number, after: number | null) => unknown;
    row: Snippet<[T, number]>;
    /** Marks a row as the one whose details are open. */
    current?: number | null;
    busy?: boolean;
  }

  let {
    items,
    key,
    label,
    sortable = false,
    onmove,
    row,
    current = null,
    busy = false,
  }: Props = $props();

  let dragging = $state<number | null>(null);
  let target = $state<{ id: number; side: 'before' | 'after' } | null>(null);

  const ids = $derived(items.map((item) => key(item)));

  function start(event: DragEvent, id: number): void {
    const transfer = event.dataTransfer;
    if (!transfer) return;
    dragging = id;
    transfer.effectAllowed = 'move';
    transfer.setData('text/plain', String(id));
    const rowElement = (event.currentTarget as HTMLElement).closest('li');
    if (rowElement) transfer.setDragImage(rowElement, 16, 16);
  }

  function over(event: DragEvent, id: number): void {
    if (dragging === null) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const side = event.clientY < box.top + box.height / 2 ? 'before' : 'after';
    if (target?.id !== id || target.side !== side) target = { id, side };
  }

  function drop(event: DragEvent, id: number): void {
    if (dragging === null) return;
    event.preventDefault();
    const moving = dragging;
    const side = target?.id === id ? target.side : 'after';
    end();
    const after = afterDropping(ids, moving, id, side);
    if (after !== undefined) onmove?.(moving, after);
  }

  function end(): void {
    dragging = null;
    target = null;
  }
</script>

<ul class="list" aria-label={label} aria-busy={busy}>
  {#each items as item, index (key(item))}
    {@const id = key(item)}
    <li
      class={[
        'row',
        {
          current: current === id,
          dragging: dragging === id,
          'drop-before': target?.id === id && target.side === 'before' && dragging !== id,
          'drop-after': target?.id === id && target.side === 'after' && dragging !== id,
        },
      ]}
      ondragover={(event) => {
        over(event, id);
      }}
      ondrop={(event) => {
        drop(event, id);
      }}
    >
      {#if sortable}
        <span
          class="grip"
          draggable="true"
          aria-hidden="true"
          ondragstart={(event) => {
            start(event, id);
          }}
          ondragend={end}
        >
          <GripVertical />
        </span>
      {/if}
      <div class="content">{@render row(item, index)}</div>
    </li>
  {/each}
</ul>

<style>
  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .row {
    position: relative;
    display: flex;
    align-items: flex-start;
  }

  .row + .row {
    border-top: 1px solid var(--color-border);
  }

  .row:first-child {
    border-start-start-radius: var(--radius-lg);
    border-start-end-radius: var(--radius-lg);
  }

  .row:last-child {
    border-end-start-radius: var(--radius-lg);
    border-end-end-radius: var(--radius-lg);
  }

  .row.current {
    background: var(--color-selected);
  }

  .row.dragging {
    opacity: 0.5;
  }

  /* Where a dragged row lands: a line on the edge it goes to. */
  .row.drop-before::before,
  .row.drop-after::after {
    content: '';
    position: absolute;
    inset-inline: var(--space-2);
    z-index: 1;
    height: 2px;
    border-radius: var(--radius-full);
    background: var(--color-focus);
  }

  .row.drop-before::before {
    top: -1px;
  }

  .row.drop-after::after {
    bottom: -1px;
  }

  .grip {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: var(--space-6);
    min-height: calc(var(--control-md) + var(--space-2));
    margin-inline-start: var(--space-1);
    color: var(--color-text-faint);
    cursor: grab;
  }

  .grip:hover {
    color: var(--color-text-muted);
  }

  .grip :global(svg) {
    width: var(--icon-md);
    height: var(--icon-md);
  }

  .content {
    flex: 1;
    min-width: 0;
  }

  /* Touch screens move rows through each row's menu. */
  @media (pointer: coarse) {
    .grip {
      display: none;
    }
  }
</style>
