<script lang="ts">
  // The title cluster of the issue page: the summary as the page's only h1, or its editor in the
  // same row while editing, with the issue's actions beside it and tags or a parent issue below.
  // The metadata line above it and the field panel beside it are the page's own layout.
  import type { Snippet } from 'svelte';

  import type { Issue } from '#lib/api/schema.gen.ts';

  import IssueSummaryField from '../IssueSummaryField.svelte';
  import * as m from '../messages.ts';

  interface Props {
    issue: Issue;
    actions?: Snippet;
    below?: Snippet;
    editing?: boolean;
    summary?: string;
    summaryError?: string | null;
    summaryInput?: HTMLInputElement;
  }

  let {
    issue,
    actions,
    below,
    editing = false,
    summary = $bindable(''),
    summaryError = null,
    summaryInput = $bindable(),
  }: Props = $props();
</script>

<header class="head">
  <div class={['row', { editing }]}>
    {#if editing}
      <h1 class="sr-only">{summary || m.noSummary()}</h1>
      <div class="edit-summary">
        <IssueSummaryField
          label={m.summary()}
          bind:value={summary}
          bind:input={summaryInput}
          error={summaryError}
          placeholder={m.summary()}
        />
      </div>
    {:else}
      <h1 class={{ untitled: !issue.summary }}>{issue.summary || m.noSummary()}</h1>
    {/if}
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

  .row.editing {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    overflow-wrap: normal;
  }

  .edit-summary {
    flex: 1;
    min-width: 0;
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

  .row.editing .actions {
    flex: none;
    margin-inline-start: 0;
  }

  .below {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
</style>
