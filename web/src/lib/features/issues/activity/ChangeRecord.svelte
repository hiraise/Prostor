<script lang="ts">
  // One record of the change history (HIST-001…HIST-004): who and when, then what changed —
  // removed values struck through and added ones underlined, each with a hidden word so the change
  // never rests on color alone (UI-032); a cleared field shows its empty label. Texts (summary,
  // description, text fields) open as a word diff on request. Inside a comment's card (HIST-011)
  // the author and time are the comment's, so `bare` leaves them out.
  import type { ChangeValues, HistoryItem, Issue } from '#lib/api/schema.gen.ts';
  import { groupName, PersonName, personName } from '#lib/features/access/index.ts';
  import { ValueChip } from '#lib/features/fields/index.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { formatNumber } from '#lib/i18n/message.ts';
  import { parseIsoDate } from '#lib/time/calendar.ts';
  import { formatDuration } from '#lib/time/duration.ts';
  import Time from '#lib/time/Time.svelte';

  import IssueRef from '../IssueRef.svelte';
  import { fieldOf } from '../issues.ts';
  import * as m from '../messages.ts';
  import DiffView from '../page/DiffView.svelte';

  interface Props {
    change: HistoryItem;
    /** The open issue: the empty labels of its fields. */
    issue: Issue;
    bare?: boolean;
  }

  let { change, issue, bare = false }: Props = $props();

  const id = $props.id();
  let diffOpen = $state(false);

  const kind = $derived(change.field?.kind ?? null);
  /** A text shown as a diff instead of struck and added values. */
  const text = $derived(
    change.kind === 'summary' || change.kind === 'description' || kind === 'text'
      ? { before: change.removed.texts[0] ?? '', after: change.added.texts[0] ?? '' }
      : null,
  );
  const textLabel = $derived(
    change.kind === 'summary'
      ? m.summaryChanged()
      : change.kind === 'description'
        ? m.descriptionChanged()
        : m.textChanged({ field: change.field?.name ?? '' }),
  );
  const label = $derived.by(() => {
    switch (change.kind) {
      case 'field':
        return change.field?.name ?? '';
      case 'project':
        return m.project();
      case 'tags':
        return m.tags();
      case 'visibility':
        return m.visibility();
      case 'attachments':
        return m.filesLabel();
      case 'links':
        return m.linksLabel();
      case 'boards':
        return m.boardsLabel();
      default:
        return '';
    }
  });
  const event = $derived(
    change.kind === 'created'
      ? m.issueCreatedChange()
      : change.kind === 'resolved'
        ? m.resolvedChange()
        : change.kind === 'reopened'
          ? m.reopenedChange()
          : change.kind === 'deleted'
            ? m.deletedChange()
            : null,
  );
  /** What a cleared value reads as: the field's empty label, or «everyone» for visibility. */
  const emptyLabel = $derived(
    change.kind === 'visibility'
      ? m.visibleToAll()
      : change.kind === 'field' && change.field
        ? (fieldOf(issue, change.field.id)?.empty_label ?? m.emptyValue())
        : null,
  );

  function isEmpty(values: ChangeValues): boolean {
    return (
      values.texts.length === 0 &&
      values.numbers.length === 0 &&
      values.values.length === 0 &&
      values.users.length === 0 &&
      values.groups.length === 0 &&
      values.tags.length === 0 &&
      values.files.length === 0 &&
      values.links.length === 0 &&
      values.boards.length === 0 &&
      values.place === null
    );
  }

  function number(value: number): string {
    return kind === 'duration'
      ? formatDuration(value, getLocale())
      : formatNumber(getLocale(), value);
  }
</script>

{#snippet values(list: ChangeValues)}
  {#each list.values as value (value.id)}
    <span class="value">
      <ValueChip name={value.name} color={value.deleted ? null : value.color} />
      {#if value.deleted}<span class="gone">({m.valueGone()})</span>{/if}
    </span>
  {/each}
  {#each list.users as user (user.id)}<span class="value"><PersonName person={user} /></span>{/each}
  {#each list.groups as group (group.id)}<span class="value">{groupName(group)}</span>{/each}
  {#each list.texts as value, index (index)}
    <span class="value">
      {#if kind === 'date' && parseIsoDate(value)}
        <Time value={parseIsoDate(value) ?? value} kind="date" />
      {:else if kind === 'date_time'}
        <Time {value} />
      {:else}
        {value}
      {/if}
    </span>
  {/each}
  {#each list.numbers as value, index (index)}<span class="value">{number(value)}</span>{/each}
  {#each list.tags as tag (tag.id)}
    <span class="value"><ValueChip name={tag.name} color={tag.color} /></span>
  {/each}
  {#each list.files as file (file.id)}<span class="value">{file.name}</span>{/each}
  {#each list.links as link (`${String(link.link_type.id)} ${link.side} ${String(link.issue.id)}`)}
    <span class="value">{link.name} <IssueRef issue={link.issue} /></span>
  {/each}
  {#each list.boards as place (`${String(place.board.id)} ${String(place.sprint?.id ?? '')}`)}
    <span class="value"
      >{place.sprint
        ? m.boardSprint({ board: place.board.name, sprint: place.sprint.name })
        : place.board.name}</span
    >
  {/each}
  {#if list.place}
    <span class="value">{list.place.project?.name ?? ''} {list.place.readable_id}</span>
  {/if}
{/snippet}

<div class={['change', { bare }]}>
  {#if !bare}
    <p class="head">
      <PersonName person={change.author} />
      {#if change.origin === 'rule'}<span class="rule">{m.ruleBadge()}</span>{/if}
      <Time value={change.at} kind="relative" />
      {#if change.silent}<span class="silent">{m.silentChange()}</span>{/if}
      <span class="sr-only">{m.changeBy({ name: personName(change.author) })}</span>
    </p>
  {/if}
  {#if change.kind === 'created' && change.clone_of?.readable && change.clone_of.issue}
    <p class="line">{m.issueClonedFrom()} <IssueRef issue={change.clone_of.issue} /></p>
  {:else if event}
    <p class="line">{event}</p>
  {:else if text}
    <div class="line">
      <span class="label">{textLabel}</span>
      <button
        type="button"
        class="link"
        aria-expanded={diffOpen}
        aria-controls="{id}-diff"
        onclick={() => (diffOpen = !diffOpen)}
      >
        {diffOpen ? m.hideDiff() : m.showDiff()}
      </button>
    </div>
    {#if diffOpen}
      <div id="{id}-diff">
        <DiffView label={textLabel} before={text.before} after={text.after} />
      </div>
    {/if}
  {:else}
    <p class="line">
      <span class="label">{label}:</span>
      {#if !isEmpty(change.removed)}
        <del
          ><span class="sr-only">{`${m.diffRemoved()} `}</span>{@render values(change.removed)}</del
        >
      {/if}
      {#if !isEmpty(change.added)}
        <ins><span class="sr-only">{`${m.diffAdded()} `}</span>{@render values(change.added)}</ins>
      {:else if emptyLabel}
        <span class="empty">{emptyLabel}</span>
      {/if}
    </p>
  {/if}
</div>

<style>
  .change {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .silent {
    font-style: italic;
  }

  /* A record a rule made carries the rule's mark on the author line (§3.12). */
  .rule {
    padding: 0 var(--space-1_5);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1_5);
    overflow-wrap: anywhere;
  }

  .label {
    color: var(--color-text-muted);
  }

  del,
  ins {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }

  del {
    text-decoration: line-through;
  }

  ins {
    text-decoration: none;
  }

  ins .value {
    text-decoration: underline;
  }

  .gone,
  .empty {
    color: var(--color-text-muted);
  }

  .link {
    min-height: 2rem;
    padding: 0 var(--space-1);
    border: none;
    background: none;
    color: var(--color-link);
    font: inherit;
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }

  @media (pointer: coarse) {
    .link {
      min-height: 2.75rem;
    }
  }
</style>
