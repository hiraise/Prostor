<script lang="ts">
  // One column of a list row (QRY-075): the ID and the summary link to the issue — a parent shows
  // how many of its subtasks are resolved (LNK-014) — fields through `FieldCell`, the other
  // attributes as text.
  import type { Issue } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { formatNumber } from '#lib/i18n/message.ts';
  import Time from '#lib/time/Time.svelte';

  import IssueRef from '../IssueRef.svelte';
  import { issueHref } from '../issues.ts';
  import * as issueMessages from '../messages.ts';
  import TagChip from '../tags/TagChip.svelte';
  import type { Column } from './columns.ts';
  import FieldCell from './FieldCell.svelte';
  import * as m from './messages.ts';

  interface Props {
    issue: Issue;
    column: Column;
    readonly?: boolean;
  }

  let { issue, column, readonly = false }: Props = $props();

  const done = $derived(issue.subtasks.total - issue.subtasks.unresolved);
</script>

{#if column.kind === 'field'}
  <FieldCell {issue} fieldId={column.id} {readonly} />
{:else if column.name === 'id'}
  <IssueRef {issue} summary={false} />
{:else if column.name === 'summary'}
  <span class="summary">
    <!-- issueHref resolves the address. -->
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
    <a href={issueHref(issue)} class={{ resolved: issue.resolved_at !== null }}>
      {#if issue.resolved_at !== null}
        <s>{issue.summary}</s><span class="sr-only"> ({issueMessages.resolved()})</span>
      {:else}{issue.summary}{/if}
    </a>
    {#if issue.subtasks.total > 0}
      <span class="progress">
        <span aria-hidden="true">{done}/{issue.subtasks.total}</span>
        <span class="sr-only">{m.progress({ done, total: issue.subtasks.total })}</span>
      </span>
    {/if}
  </span>
{:else if column.name === 'project'}
  {issue.project.name}
{:else if column.name === 'reporter'}
  <PersonName person={issue.reporter} />
{:else if column.name === 'created'}
  <Time value={issue.created_at} kind="relative" />
{:else if column.name === 'updated'}
  <Time value={issue.updated_at} kind="relative" />
{:else if column.name === 'resolved'}
  {#if issue.resolved_at !== null}<Time value={issue.resolved_at} kind="relative" />{/if}
{:else if column.name === 'votes'}
  {formatNumber(getLocale(), issue.votes)}
{:else}
  <span class="tags">
    {#each issue.tags as tag (tag.id)}<TagChip {tag} />{/each}
  </span>
{/if}

<style>
  .summary {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
  }

  .summary a {
    color: var(--color-link);
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  .summary a:hover {
    text-decoration: underline;
  }

  .summary a.resolved {
    color: var(--color-text-muted);
  }

  .progress {
    padding: 0 var(--space-1);
    border-radius: var(--radius-sm);
    background: var(--color-hover);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .tags {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
</style>
