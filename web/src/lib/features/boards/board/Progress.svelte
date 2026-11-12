<script lang="ts">
  // Progress of the shown sprint (BRD-085): one part per column, as wide as its share of the
  // cards; a column over or under its limits is red with an icon. Each part says its column and
  // count on hover and on focus, and the whole bar is a list a screen reader reads out.
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import type { Board, ColumnCount } from '#lib/api/schema.gen.ts';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import * as m from '../messages.ts';

  interface Props {
    board: Board;
    columns: ColumnCount[];
  }

  let { board, columns }: Props = $props();

  const counts = $derived(new Map(columns.map((one) => [one.column, one])));
  const parts = $derived(
    board.columns.flatMap((column) => {
      const count = counts.get(column.id);
      return count ? [{ column, count }] : [];
    }),
  );
  const total = $derived(parts.reduce((sum, part) => sum + part.count.count, 0));
</script>

{#if parts.length > 0}
  <ul class="progress" aria-label={m.progress()}>
    {#each parts as { column, count }, index (column.id)}
      {@const label = m.progressPart({ name: column.value.name, count: count.count })}
      {@const broken = count.wip !== 'within'}
      <!-- Focus shows the part's tooltip to keyboard users. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <li
        class={['part', { broken, done: column.value.resolved }]}
        style:flex-grow={total === 0 ? 1 : Math.max(count.count, 0.0001)}
        style:--shade={String(index % 4)}
        tabindex="0"
        {@attach tooltip(label)}
      >
        <span class="sr-only">{label}</span>
        {#if broken}<TriangleAlert size={12} aria-hidden="true" />{/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .progress {
    display: flex;
    gap: 2px;
    width: min(16rem, 100%);
    height: 0.75rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .part {
    display: flex;
    flex-basis: 0;
    align-items: center;
    justify-content: center;
    min-width: 0.375rem;
    border-radius: var(--radius-sm);
    background: color-mix(
      in srgb,
      var(--color-brand) calc(30% + var(--shade) * 15%),
      var(--color-surface-sunken)
    );
    color: var(--color-on-danger);
  }

  .part.done {
    background: var(--color-success);
  }

  .part.broken {
    min-width: 1rem;
    background: var(--color-danger-solid);
  }

  .part:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }

  @media (forced-colors: active) {
    .part {
      border: 1px solid CanvasText;
    }
  }
</style>
