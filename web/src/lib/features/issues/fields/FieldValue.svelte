<script lang="ts">
  // The value of one field (ISS-020, FLD-044): chips in their colors for set values, people with
  // their avatars, dates and periods in the viewer's format, and the field's own label, dimmed,
  // when it is empty.
  import type { IssueField } from '#lib/api/schema.gen.ts';
  import { groupName, PersonName } from '#lib/features/access/index.ts';
  import { ValueChip } from '#lib/features/fields/index.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { formatNumber } from '#lib/i18n/message.ts';
  import { parseIsoDate } from '#lib/time/calendar.ts';
  import { formatDuration } from '#lib/time/duration.ts';
  import Time from '#lib/time/Time.svelte';

  import { isEmpty } from './field-values.ts';

  let { entry }: { entry: IssueField } = $props();

  const date = $derived(entry.date === null ? null : parseIsoDate(entry.date));
</script>

{#if isEmpty(entry)}
  <span class="empty">{entry.empty_label}</span>
{:else if entry.field.kind === 'string'}
  <span class="text">{entry.text}</span>
{:else if entry.field.kind === 'integer' && entry.integer !== null}
  {formatNumber(getLocale(), entry.integer)}
{:else if entry.field.kind === 'float' && entry.float !== null}
  {formatNumber(getLocale(), entry.float)}
{:else if entry.field.kind === 'date' && date}
  <Time value={date} kind="date" />
{:else if entry.field.kind === 'date_time' && entry.date_time !== null}
  <Time value={entry.date_time} />
{:else if entry.field.kind === 'duration' && entry.minutes !== null}
  {formatDuration(entry.minutes, getLocale())}
{:else if entry.field.kind === 'user'}
  <span class="list">
    {#each entry.users as user (user.id)}<PersonName person={user} />{/each}
  </span>
{:else if entry.field.kind === 'group'}
  <span class="list">
    {#each entry.groups as group (group.id)}<span>{groupName(group)}</span>{/each}
  </span>
{:else}
  <span class="list">
    {#each entry.values as value (value.id)}<ValueChip
        name={value.name}
        color={value.color}
      />{/each}
  </span>
{/if}

<style>
  .empty {
    color: var(--color-text-muted);
  }

  .text {
    overflow-wrap: anywhere;
  }

  .list {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    min-width: 0;
  }
</style>
