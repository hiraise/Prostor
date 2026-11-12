<script lang="ts">
  // An issue wherever it is named (ISS-024, ISS-042): its ID and summary, a lock when it is limited
  // to an audience, and a resolved issue muted with its ID struck through (and said in words).
  import Lock from '@lucide/svelte/icons/lock';

  import type { Issue, IssueBrief } from '#lib/api/schema.gen.ts';

  import { audienceText } from './audience/audience.ts';
  import { briefOf, issueHref } from './issues.ts';
  import * as m from './messages.ts';

  interface Props {
    issue: IssueBrief | Issue;
    /** Shows the summary after the ID. */
    summary?: boolean;
    /** Links to the issue page. */
    link?: boolean;
  }

  let { issue, summary = true, link = true }: Props = $props();

  const brief = $derived(briefOf(issue));
  const lockText = $derived(
    'audience' in issue ? (audienceText(issue.audience) ?? m.restricted()) : m.restrictedHidden(),
  );
</script>

{#snippet body()}
  {#if brief.resolved}
    <s class="id">{brief.readable_id}</s><span class="sr-only"> ({m.resolved()})</span>
  {:else}
    <span class="id">{brief.readable_id}</span>
  {/if}
  {#if summary}<span class="summary">{brief.summary}</span>{/if}
  {#if brief.restricted}
    <span class="lock" role="img" aria-label={lockText}><Lock size={14} aria-hidden="true" /></span>
  {/if}
{/snippet}

{#if link}
  <!-- issueHref resolves the address. -->
  <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
  <a class={['ref', { resolved: brief.resolved }]} href={issueHref(brief.readable_id)}>
    {@render body()}
  </a>
{:else}
  <span class={['ref', { resolved: brief.resolved }]}>{@render body()}</span>
{/if}

<style>
  .ref {
    display: inline;
    color: inherit;
    overflow-wrap: anywhere;
  }

  a.ref {
    color: var(--color-link);
    text-decoration: none;
  }

  a.ref:hover .summary,
  a.ref:hover .id {
    text-decoration: underline;
  }

  .id {
    font-family: var(--font-mono);
    font-size: 0.9em;
    white-space: nowrap;
  }

  .summary {
    margin-inline-start: var(--space-1);
  }

  .resolved,
  a.ref.resolved {
    color: var(--color-text-muted);
  }

  .lock {
    display: inline-flex;
    margin-inline-start: var(--space-1);
    vertical-align: -0.125em;
  }
</style>
