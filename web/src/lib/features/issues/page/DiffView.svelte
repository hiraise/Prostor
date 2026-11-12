<script lang="ts">
  // A word diff (ISS-051): removed words struck through, added ones underlined, each with a hidden
  // word for screen readers, so the change never rests on color alone (UI-032).
  import * as m from '../messages.ts';
  import { diffWords } from './diff.ts';

  let { before, after, label }: { before: string; after: string; label: string } = $props();

  const parts = $derived(diffWords(before, after));
  const changed = $derived(parts.some((part) => part.op !== 'same'));
</script>

<figure class="diff">
  <figcaption>{label}</figcaption>
  {#if changed}
    <p class="text">
      {#each parts as part, index (index)}
        {#if part.op === 'removed'}<del
            ><span class="sr-only">{`${m.diffRemoved()} `}</span>{part.text}</del
          >{:else if part.op === 'added'}<ins
            ><span class="sr-only">{`${m.diffAdded()} `}</span>{part.text}</ins
          >{:else}<span>{part.text}</span>{/if}
      {/each}
    </p>
  {:else}
    <p class="text muted">{m.noChanges()}</p>
  {/if}
</figure>

<style>
  .diff {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
  }

  figcaption {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .text {
    max-height: 16rem;
    overflow: auto;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .muted {
    color: var(--color-text-muted);
  }

  del {
    background: var(--color-danger-bg);
    text-decoration: line-through;
  }

  ins {
    background: var(--color-success-bg);
    text-decoration: underline;
  }
</style>
