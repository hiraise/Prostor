<script lang="ts">
  // The top of the issue page: the readable ID with a copy button (UI-006), the summary as the
  // page's only h1, and the issue's actions at the end.
  import Copy from '@lucide/svelte/icons/copy';
  import type { Snippet } from 'svelte';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import { copyText } from '#lib/ui/copy.ts';
  import IconButton from '#lib/ui/IconButton.svelte';

  import * as m from '../messages.ts';

  let { issue, actions }: { issue: Issue; actions?: Snippet } = $props();

  const readable = $derived(issue.readable_id);
</script>

<header class="header">
  <div class="title">
    <p class="id">
      {#if readable}
        <span>{readable}</span>
        <IconButton
          icon={Copy}
          label={m.copyId()}
          onclick={() => copyText(readable, m.idCopied({ id: readable }))}
        />
      {:else}
        <span class="draft">{m.draft()}</span>
      {/if}
    </p>
    <h1 class={{ untitled: !issue.summary }}>{issue.summary || m.noSummary()}</h1>
  </div>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</header>

<style>
  .header {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
  }

  .title {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    min-width: 0;
    flex: 1 1 20rem;
  }

  .id {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  h1 {
    font-size: var(--text-xl);
    overflow-wrap: anywhere;
  }

  .untitled {
    color: var(--color-text-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
