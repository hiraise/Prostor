<script lang="ts">
  // The title cluster of the issue page: the summary as the page's only h1 with the issue's
  // actions as icons flowing right after its text, and under the title whatever the page hangs
  // there (tags, the parent issue). The metadata line above it and the field panel beside it are
  // the page's own layout (`IssuePage`).
  import type { Snippet } from 'svelte';

  import type { Issue } from '#lib/api/schema.gen.ts';

  import * as m from '../messages.ts';

  let { issue, actions, below }: { issue: Issue; actions?: Snippet; below?: Snippet } = $props();
</script>

<header class="head">
  <div class="row">
    <h1 class={{ untitled: !issue.summary }}>{issue.summary || m.noSummary()}</h1>
    {#if actions}<span class="actions">{@render actions()}</span>{/if}
  </div>
  {#if below}<div class="below">{@render below()}</div>{/if}
</header>

<style>
  .head {
    display: flex;
    flex-direction: column;
    gap: var(--space-1_5);
  }

  /* The actions flow right after the summary's text, wrapping with it. */
  .row {
    overflow-wrap: anywhere;
  }

  h1 {
    display: inline;
    margin: 0;
    font-size: var(--text-2xl);
    line-height: 1.25;
  }

  .untitled {
    color: var(--color-text-muted);
  }

  .actions {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    margin-inline-start: var(--space-1);
    vertical-align: middle;
  }

  .below {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
</style>
