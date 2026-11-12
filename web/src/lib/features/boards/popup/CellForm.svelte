<script lang="ts">
  // Creating in a cell (BRD-077): the «+» of a cell opens this form in place. It edits the
  // person's draft of the cell, which the server keeps per cell: the cell's column and lane values
  // are set on it and locked, inside a parent lane it is that issue's subtask. «Добавить» (or
  // Ctrl/⌘+Enter) publishes it, «Добавить и открыть» also opens it in the popup. Esc or «Отмена»
  // closes the form: an empty draft is deleted, any other kept for the next «+» of the cell.
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Lock from '@lucide/svelte/icons/lock';
  import { onMount, tick } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { Board, BoardColumn } from '#lib/api/schema.gen.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { DraftSession, FieldPanel } from '#lib/features/issues/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextArea from '#lib/ui/TextArea.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import type { BoardState } from '../board/board-state.svelte.ts';
  import * as m from '../messages.ts';

  interface Props {
    board: Board;
    column: BoardColumn;
    lane: string | null;
    model: BoardState;
    onclose: () => unknown;
  }

  let { board, column, lane, model, onclose }: Props = $props();

  const id = $props.id();
  const session = new DraftSession();
  let starting = $state(true);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let invalid = $state<number[]>([]);
  let fieldsOpen = $state(false);
  let summaryInput = $state<HTMLInputElement>();
  let form = $state<HTMLFormElement>();

  const issue = $derived(session.issue);
  const parentLane = $derived(board.lanes.mode === 'parents' && lane !== null && lane !== 'other');
  /** The fields the cell sets: the column field, and the lane field of value lanes. */
  const locked = $derived(
    [
      board.column_field?.id,
      board.lanes.mode === 'values' && lane !== 'other' ? board.lanes.field?.id : undefined,
    ].filter((one): one is number => one !== undefined),
  );
  const laneName = $derived(lane === null ? '' : model.laneLabel(lane).name);
  const canAdd = $derived(session.summary.trim() !== '' && !busy);

  onMount(() => {
    void start();
    return () => {
      // Leaving the board keeps a draft with content, deletes an empty one.
      if (session.id !== null) void session.close().catch(() => undefined);
    };
  });

  async function start(): Promise<void> {
    const draft = await model.cellDraft(column.id, lane);
    const found = draft !== null && (await session.open(draft).catch(() => false));
    starting = false;
    if (!found) {
      onclose();
      return;
    }
    await tick();
    summaryInput?.focus();
  }

  async function add(open: boolean): Promise<void> {
    if (!canAdd) return;
    busy = true;
    problem = null;
    invalid = [];
    try {
      const created = await session.publish();
      toasts.show({
        tone: 'success',
        message: m.cardCreated({ id: created.readable_id ?? String(created.id) }),
      });
      onclose();
      void model.load();
      if (open) model.openIssue(created.id);
    } catch (error) {
      problem = refusalText(error);
      if (error instanceof ApiError && error.code === 'invalid_request') {
        invalid = (error.body?.fields ?? [])
          .map((entry) => /^fields\.(\d+)$/.exec(entry.path)?.[1])
          .filter((value): value is string => value !== undefined)
          .map(Number);
        if (invalid.length > 0) {
          problem = m.fillRequired();
          fieldsOpen = true;
        }
      }
      announce(problem, 'assertive');
    } finally {
      busy = false;
    }
  }

  async function cancel(): Promise<void> {
    busy = true;
    const result = await session.close().catch(() => 'kept' as const);
    if (result === 'kept') toasts.show({ tone: 'info', message: m.draftKept() });
    onclose();
  }

  function keydown(event: KeyboardEvent): void {
    if (event.defaultPrevented) return;
    if (event.key === 'Escape' && !document.querySelector(':popover-open')) {
      event.preventDefault();
      event.stopPropagation();
      void cancel();
    } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void add(false);
    }
  }
</script>

{#if starting}
  <div class="starting"><Spinner /></div>
{:else if issue}
  <!-- Esc cancels, Ctrl/⌘+Enter adds. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <form
    class="form"
    aria-label={m.cellForm()}
    bind:this={form}
    onsubmit={(event) => {
      event.preventDefault();
      void add(false);
    }}
    onkeydown={keydown}
    onfocusout={() => void session.flush().catch(() => undefined)}
  >
    <TextField
      label={m.summaryLabel()}
      bind:value={session.summary}
      bind:input={summaryInput}
      required
      maxlength={255}
      oninput={() => {
        session.typed();
      }}
    />
    <TextArea
      label={m.descriptionLabel()}
      bind:value={session.description}
      rows={3}
      oninput={() => {
        session.typed();
      }}
    />
    <ul class="place">
      <li>
        <Lock size={12} aria-hidden="true" />
        <span class="sr-only">{m.setByCell()}:</span>
        {m.lockedPlace({ name: board.column_field?.name ?? '', value: column.value.name })}
      </li>
      {#if lane !== null && lane !== 'other'}
        <li>
          <Lock size={12} aria-hidden="true" />
          <span class="sr-only">{m.setByCell()}:</span>
          {parentLane
            ? m.subtaskOfLane({ name: laneName })
            : m.lockedPlace({ name: board.lanes.field?.name ?? '', value: laneName })}
        </li>
      {/if}
    </ul>
    <button
      type="button"
      class="more"
      aria-expanded={fieldsOpen}
      aria-controls="{id}-fields"
      onclick={() => (fieldsOpen = !fieldsOpen)}
    >
      <ChevronRight size={14} aria-hidden="true" class={fieldsOpen ? 'open' : ''} />{m.moreFields()}
    </button>
    <div id="{id}-fields" hidden={!fieldsOpen}>
      {#if fieldsOpen}
        <FieldPanel
          {issue}
          collapsed={false}
          side="end"
          ontoggle={() => undefined}
          fixed
          {invalid}
          locked={{ ids: locked, label: m.setByCell() }}
        />
      {/if}
    </div>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <div class="buttons">
      <Button type="submit" size="sm" variant="primary" loading={busy} disabled={!canAdd}
        >{m.addCardButton()}</Button
      >
      <Button size="sm" disabled={!canAdd} onclick={() => add(true)}>{m.addAndOpen()}</Button>
      <Button size="sm" variant="ghost" disabled={busy} onclick={cancel}>{m.cancel()}</Button>
    </div>
  </form>
{/if}

<style>
  .starting {
    display: flex;
    justify-content: center;
    padding: var(--space-2);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    box-shadow: var(--shadow-sm);
  }

  .place {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    list-style: none;
  }

  .place li {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .more {
    display: inline-flex;
    align-items: center;
    align-self: flex-start;
    gap: var(--space-1);
    padding: 0;
    border: 0;
    background: none;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .more :global(.open) {
    transform: rotate(90deg);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1_5);
  }
</style>
