<script lang="ts">
  // Adding an issue to a query board (BRD-028, owner decision 2): the field changes that make it
  // match the board query, as «поле: было → станет»; nothing changes until «Добавить».
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';

  import * as m from '../messages.ts';
  import type { ChangeLine } from './query-mode.ts';

  interface Props {
    open?: boolean;
    /** The issue's ID as people read it. */
    id: string;
    lines: readonly ChangeLine[];
    onconfirm: () => Promise<unknown>;
  }

  let { open = $bindable(false), id, lines, onconfirm }: Props = $props();

  let busy = $state(false);

  async function confirm(): Promise<void> {
    busy = true;
    try {
      await onconfirm();
      open = false;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.queryAddTitle({ id })} size="sm" dismissible={!busy}>
  {#if lines.length === 0}
    <p>{m.queryAddNothing()}</p>
  {:else}
    <p>{m.queryAddText()}</p>
    <ul class="changes">
      {#each lines as line, index (index)}
        <li>
          <span class="sr-only">{m.changeLine(line)}</span>
          <span class="field" aria-hidden="true">{line.field}:</span>
          <span class="was" aria-hidden="true">{line.was}</span>
          <span aria-hidden="true">→</span>
          <strong aria-hidden="true">{line.now}</strong>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet footer()}
    <Button variant="ghost" disabled={busy} onclick={() => (open = false)}>{m.cancel()}</Button>
    <Button variant="primary" loading={busy} onclick={() => void confirm()}>{m.queryAdd()}</Button>
  {/snippet}
</Dialog>

<style>
  .changes {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-3);
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    align-items: baseline;
  }

  .field {
    color: var(--color-text-muted);
  }

  .was {
    text-decoration: line-through;
    color: var(--color-text-muted);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
