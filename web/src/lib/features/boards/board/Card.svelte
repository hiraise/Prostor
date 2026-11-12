<script lang="ts">
  // A card (BRD-058, ISS-024) in four sizes. Every size: the ID (struck through when resolved),
  // the summary (two lines on the smaller sizes, three on the larger), the assignee's avatar and
  // the estimation. Medium and up add the fields of the «Карточка» settings (BRD-059) and the
  // file count, large and up the tags, the largest the start of the description. A colored stripe
  // on the leading edge shows the project or the value of the color field (BRD-060), always
  // with a name beside it. People who may change the issue edit fields, the assignee and the
  // estimation in place and take tags off (BRD-063). A restricted issue shows a lock (ISS-042).
  // The ID link covers the card: a click, Enter or a double click opens the issue in the board's
  // popup (BRD-079), F2 in its edit mode (BRD-080); Ctrl/⌘ or the middle button keep the link.
  import Lock from '@lucide/svelte/icons/lock';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import X from '@lucide/svelte/icons/x';
  import type { Snippet } from 'svelte';

  import type {
    Board,
    CardField,
    CardIssue,
    FieldRef,
    IssueField,
    ProjectRef,
  } from '#lib/api/schema.gen.ts';
  import {
    FieldEditor,
    IssueRef,
    sendField,
    type FieldRequest,
  } from '#lib/features/issues/index.ts';
  import { ValueChip } from '#lib/features/fields/index.ts';
  import Avatar from '#lib/ui/Avatar.svelte';
  import Popover from '#lib/ui/Popover.svelte';

  import { plainExcerpt, type CardSize } from '../boards.ts';
  import * as m from '../messages.ts';
  import type { BoardState } from './board-state.svelte.ts';
  import { cardFieldOf, entryOf, formatNumber } from './card-fields.ts';
  import CardMenu from './CardMenu.svelte';

  interface Props {
    issue: CardIssue;
    board: Board;
    model: BoardState;
    size: CardSize;
  }

  let { issue, board, model, size }: Props = $props();

  const project = $derived<ProjectRef>(
    board.projects.find((one) => one.project.id === issue.project)?.project ?? {
      id: issue.project,
      name: '',
      key: '',
      archived: false,
      logo_color: 0,
      logo_url: null,
    },
  );
  const brief = $derived({
    id: issue.id,
    readable_id: issue.readable_id ?? String(issue.id),
    summary: issue.summary,
    project,
    resolved: issue.resolved_at !== null,
    restricted: false,
  });
  const valueOf = (id: number | undefined): CardField | undefined =>
    issue.fields.find((one) => one.field === id);
  const assigneeField = $derived(
    [...model.fields.values()].find((one) => one.builtin === 'assignee'),
  );
  const assignee = $derived.by(() => {
    const id = valueOf(assigneeField?.id)?.ids[0];
    return id === undefined ? undefined : model.users.get(id);
  });
  const estimationRef = $derived(board.cards.estimation);
  const estimation = $derived(valueOf(estimationRef?.id)?.number ?? null);
  /** The «Карточка» fields; empty ones only for people who may set them. */
  const shown = $derived(
    board.cards.fields.filter(
      (ref) => ref.kind !== 'text' && (issue.can_update || valueOf(ref.id) !== undefined),
    ),
  );
  /** The stripe: a color scheme and the name it stands for. */
  const stripe = $derived.by((): { color: number; name: string } | null => {
    if (board.cards.color === 'project') {
      const color = board.projects.find((one) => one.project.id === issue.project)?.color;
      return color == null ? null : { color, name: project.name };
    }
    if (board.cards.color === 'field') {
      const id = valueOf(board.cards.color_field?.id)?.ids[0];
      const value = id === undefined ? undefined : model.values.get(id);
      return value?.color == null ? null : { color: value.color, name: value.name };
    }
    return null;
  });
  /** The color field's value is named on the card unless a shown field names it already. */
  const stripeNamed = $derived(
    board.cards.color !== 'field' || shown.some((ref) => ref.id === board.cards.color_field?.id),
  );
  const excerpt = $derived(size === 3 ? plainExcerpt(issue.excerpt) : '');

  function text(ref: FieldRef, field: CardField): string {
    if (field.number !== null) return formatNumber(field.number, ref.kind);
    if (field.ids.length > 0) return model.valuesOf(field).join(', ');
    return field.text ?? '';
  }

  function initial(name: string): string {
    const [first] = new Intl.Segmenter().segment(name.trim());
    return first?.segment.toLocaleUpperCase() ?? '';
  }

  function entry(ref: FieldRef): IssueField {
    return entryOf(ref, valueOf(ref.id), issue.can_update, model);
  }

  /** A plain click on the ID link (the card's body) opens the popup instead of the page. */
  function click(event: MouseEvent): void {
    if (event.defaultPrevented || event.button !== 0) return;
    if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const target = event.target instanceof Element ? event.target : null;
    if (!target?.closest('.id a')) return;
    event.preventDefault();
    model.openIssue(issue.id);
  }

  function keydown(event: KeyboardEvent): void {
    if (event.key !== 'F2' || event.defaultPrevented) return;
    if (!(event.target instanceof Element) || !event.target.closest('.id a')) return;
    event.preventDefault();
    model.openIssue(issue.id, issue.can_update);
  }

  function save(next: IssueField, request: FieldRequest): void {
    const { field, refs } = cardFieldOf(next);
    void model.editField(issue.id, field, refs, () => sendField(issue.id, next.field.id, request));
  }
</script>

{#snippet editable(ref: FieldRef, shownValue: Snippet, className: string)}
  {#if issue.can_update && ref.kind !== 'text'}
    <Popover label={m.fieldEditor({ name: ref.name })}>
      {#snippet trigger(props)}
        <button type="button" class={['edit', className]} {...props}>
          <span class="sr-only"
            >{`${m.editField({ name: ref.name, id: brief.readable_id })}: `}</span
          >
          {@render shownValue()}
        </button>
      {/snippet}
      {#snippet children({ close })}
        <FieldEditor issueId={issue.id} entry={entry(ref)} onsave={save} {close} />
      {/snippet}
    </Popover>
  {:else}
    <span class={className}>{@render shownValue()}</span>
  {/if}
{/snippet}

<!-- The ID link inside is the control; these handlers only change where it leads. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article
  class={['card', `size-${String(size)}`, { striped: stripe !== null }]}
  onclick={click}
  onkeydown={keydown}
  style:--stripe={stripe ? `var(--value-${String(stripe.color)}-fg)` : undefined}
  aria-labelledby="card-{issue.id}"
>
  <div class="top">
    <span class="id" id="card-{issue.id}"><IssueRef issue={brief} summary={false} /></span>
    {#if issue.restricted}
      <span class="lock" role="img" aria-label={m.restricted()} title={m.restricted()}
        ><Lock size={14} aria-hidden="true" /></span
      >
    {/if}
    {#if stripe}
      {#if stripeNamed}
        <span class="sr-only">{m.colorOf({ name: stripe.name })}</span>
      {:else}
        <span class="stripe-name"><ValueChip name={stripe.name} color={stripe.color} /></span>
      {/if}
    {/if}
    <span class="tools">
      {#if assigneeField && (assignee !== undefined || issue.can_update)}
        {#snippet person()}
          {#if assignee}
            <span class="avatar" role="img" aria-label={m.assignee({ name: assignee.full_name })}>
              <Avatar
                name={assignee.full_name}
                size="sm"
                color={assignee.avatar_color}
                src={assignee.avatar_url}
              />
            </span>
          {:else}
            <UserPlus size={16} aria-hidden="true" />
            <span class="sr-only">{m.emptyField({ name: assigneeField.name })}</span>
          {/if}
        {/snippet}
        {@render editable(assigneeField, person, 'person')}
      {/if}
      <span class="menu"><CardMenu {issue} {board} {model} /></span>
    </span>
  </div>
  <p class="summary">{issue.summary}</p>
  {#if size >= 1 && (shown.length > 0 || issue.files > 0)}
    <dl class="fields">
      {#each shown as ref (ref.id)}
        {@const field = valueOf(ref.id)}
        {@const short = board.cards.initials.includes(ref.id)}
        <div class="field">
          <dt class="sr-only">{ref.name}</dt>
          <dd>
            {#snippet value()}
              {#if !field}
                <span class="plain empty">{m.emptyField({ name: ref.name })}</span>
              {:else if field.ids.length > 0}
                {#each field.ids as id, index (id)}
                  {@const held = model.values.get(id)}
                  {@const name = held?.name ?? model.valuesOf(field)[index] ?? ''}
                  {#if short}
                    <span class="initial" title={name}
                      ><span aria-hidden="true">{initial(name)}</span><span class="sr-only"
                        >{name}</span
                      ></span
                    >
                  {:else if held}
                    <ValueChip name={held.name} color={held.color} />
                  {:else}
                    <span class="plain">{name}</span>
                  {/if}
                {/each}
              {:else if short}
                <span class="initial" title={text(ref, field)}
                  ><span aria-hidden="true">{initial(text(ref, field))}</span><span class="sr-only"
                    >{text(ref, field)}</span
                  ></span
                >
              {:else}
                <span class="plain">{text(ref, field)}</span>
              {/if}
            {/snippet}
            {@render editable(ref, value, 'value')}
          </dd>
        </div>
      {/each}
      {#if issue.files > 0}
        <div class="field">
          <dt class="sr-only">{m.filesLabel()}</dt>
          <dd class="plain files">
            <Paperclip size={14} aria-hidden="true" />{m.files({ count: issue.files })}
          </dd>
        </div>
      {/if}
    </dl>
  {/if}
  {#if size >= 2 && issue.tags.length > 0}
    <ul class="tags" aria-label={m.tagsLabel()}>
      {#each issue.tags as tag (tag.id)}
        <li>
          <ValueChip name={tag.name} color={tag.color} />
          {#if issue.can_update}
            <button
              type="button"
              class="untag"
              aria-label={m.removeTag({ name: tag.name })}
              onclick={() => {
                void model.removeTag(issue.id, tag.id);
              }}><X size={12} aria-hidden="true" /></button
            >
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if excerpt}<p class="excerpt">{excerpt}</p>{/if}
  {#if estimationRef && (estimation !== null || issue.can_update)}
    {#snippet estimate()}
      <span>{estimation === null ? '—' : formatNumber(estimation, estimationRef.kind)}</span>
    {/snippet}
    <p class="estimation">
      {#if !issue.can_update}<span class="sr-only">{`${estimationRef.name}: `}</span>{/if}
      {@render editable(estimationRef, estimate, 'number')}
    </p>
  {/if}
</article>

<style>
  .card {
    position: relative;
    contain: layout paint;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-sm);
    font-size: var(--text-sm);
  }

  /* The color stripe (BRD-060) on the leading edge; the name is beside it or in a field. */
  .striped {
    border-inline-start: 4px solid var(--stripe);
  }

  .card:hover {
    border-color: var(--color-border-control);
  }

  .card:focus-within {
    outline: 2px solid var(--color-focus);
    outline-offset: 1px;
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: 1.5rem;
  }

  .id {
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
  }

  /* The ID link covers the card; the focus ring sits on the card instead. */
  .id :global(a) {
    outline: none;
  }

  .id :global(a)::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
  }

  .lock {
    display: inline-flex;
    color: var(--color-text-muted);
  }

  /* Narrow cards (the phone overview) shrink the name so the tools stay on the card. */
  .stripe-name {
    min-width: 0;
    margin-inline-end: auto;
    overflow: hidden;
  }

  /* Editors and tag removal sit above the link that covers the card. */
  .edit,
  .untag {
    position: relative;
    z-index: 1;
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
    min-width: 1.5rem;
    min-height: 1.5rem;
    padding: 0 var(--space-1);
    margin-inline: calc(-1 * var(--space-1));
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .edit:hover,
  .untag:hover {
    border-color: var(--color-border);
    background: var(--color-hover);
  }

  .untag {
    margin-inline: 0;
    padding: 0 0.125rem;
    color: var(--color-text-muted);
  }

  .tags li {
    display: inline-flex;
    align-items: center;
  }

  .initial {
    display: inline-grid;
    place-items: center;
    min-width: 1.25rem;
    height: 1.25rem;
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }

  .empty {
    font-style: italic;
  }

  .files {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  @media (pointer: coarse) {
    .edit,
    .untag {
      min-height: 2.75rem;
      min-width: 2.75rem;
    }
  }

  .tools {
    display: inline-flex;
    flex-shrink: 0;
    margin-inline-start: auto;
    align-items: center;
    gap: var(--space-1);
  }

  .avatar {
    display: inline-flex;
    flex-shrink: 0;
  }

  /* Above the link that covers the card; on a mouse it shows with the card's hover or focus. */
  .menu {
    position: relative;
    z-index: 1;
    display: inline-flex;
  }

  @media (pointer: fine) {
    .menu {
      opacity: 0;
    }

    .card:hover .menu,
    .card:focus-within .menu {
      opacity: 1;
    }
  }

  .summary {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    color: var(--color-text);
    overflow-wrap: anywhere;
  }

  .size-2 .summary,
  .size-3 .summary {
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .fields,
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .field dd {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin: 0;
  }

  .plain {
    color: var(--color-text-muted);
  }

  .excerpt {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    color: var(--color-text-muted);
    white-space: pre-line;
    overflow-wrap: anywhere;
  }

  .estimation {
    align-self: flex-end;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .size-0 {
    padding: var(--space-1) var(--space-2);
  }

  @media (forced-colors: active) {
    .card {
      border-color: CanvasText;
    }
  }
</style>
