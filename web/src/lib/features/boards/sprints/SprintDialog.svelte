<script lang="ts">
  // Creating and editing a sprint (BRD-098, BRD-102): name, goal and dates. A new sprint may carry
  // the unresolved issues of the current one over (BRD-101) and become the sprint new issues join
  // (BRD-026). Editing also archives or restores it (BRD-103) and deletes it after a confirmation
  // (BRD-104).
  import { untrack } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { Sprint } from '#lib/api/schema.gen.ts';
  import { refusal } from '#lib/features/fields/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextArea from '#lib/ui/TextArea.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { createSprint, deleteSprint, sprintAction, updateSprint } from '../boards.ts';
  import * as m from '../messages.ts';

  interface Props {
    open?: boolean;
    board: { id: number };
    /** The sprint to edit; `null` creates one. */
    sprint?: Sprint | null;
    /** The sprint whose unresolved issues a new one may take; `null` hides the option. */
    carryFrom?: Sprint | null;
    /** A sprint was created or changed. */
    onsaved: (sprint: Sprint, created: boolean) => unknown;
    /** The sprint was deleted. */
    ondeleted?: (sprint: Sprint) => unknown;
    /** Opens on the delete confirmation (the footer of an archived sprint). */
    deleting?: boolean;
  }

  let {
    open = $bindable(false),
    board,
    sprint = null,
    carryFrom = null,
    onsaved,
    ondeleted,
    deleting = false,
  }: Props = $props();

  let name = $state('');
  let goal = $state('');
  let starts = $state('');
  let ends = $state('');
  let carry = $state(false);
  let auto = $state(false);
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let busy = $state(false);
  let confirming = $state(false);
  const formId = $props.id();

  // Each opening starts from the sprint (or empty).
  $effect(() => {
    if (!open) return;
    untrack(() => {
      name = sprint?.name ?? '';
      goal = sprint?.goal ?? '';
      starts = sprint?.starts_on ?? '';
      ends = sprint?.ends_on ?? '';
      carry = false;
      auto = false;
      errors = {};
      problem = null;
      confirming = deleting && sprint !== null;
    });
  });

  const PATHS = ['name', 'goal', 'starts_on', 'ends_on'];

  function check(): boolean {
    const found: Record<string, string> = {};
    if (name.trim() === '') found.name = m.sprintNameNeeded();
    if (starts !== '' && ends !== '' && ends < starts) found.ends_on = m.sprintEndsBeforeStart();
    errors = found;
    return Object.keys(found).length === 0;
  }

  function fail(error: unknown): void {
    if (error instanceof ApiError && error.code === 'conflict') {
      problem = m.sprintChanged();
      return;
    }
    const found = refusal(error, { paths: PATHS });
    errors = found.fields;
    problem = found.message ?? (Object.keys(found.fields).length > 0 ? null : m.sprintSaveFailed());
  }

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy || !check()) return;
    busy = true;
    problem = null;
    const dates = { starts_on: starts === '' ? null : starts, ends_on: ends === '' ? null : ends };
    try {
      if (sprint) {
        const saved = await updateSprint(board.id, sprint.id, {
          name: name.trim(),
          goal: goal.trim(),
          ...dates,
          version: sprint.version,
        });
        open = false;
        onsaved(saved, false);
      } else {
        const created = await createSprint(board.id, {
          name: name.trim(),
          goal: goal.trim(),
          ...dates,
          ...(carry && carryFrom ? { carry_from: carryFrom.id } : {}),
          ...(auto ? { auto_target: true } : {}),
        });
        open = false;
        toasts.show({ tone: 'success', message: m.sprintCreated({ name: created.name }) });
        onsaved(created, true);
      }
    } catch (error) {
      fail(error);
    } finally {
      busy = false;
    }
  }

  async function archive(): Promise<void> {
    if (!sprint || busy) return;
    busy = true;
    try {
      const changed = await sprintAction(
        board.id,
        sprint.id,
        sprint.archived ? 'restore' : 'archive',
      );
      open = false;
      toasts.show({
        tone: 'success',
        message: changed.archived ? m.sprintArchived() : m.sprintRestored(),
      });
      onsaved(changed, false);
    } catch (error) {
      fail(error);
    } finally {
      busy = false;
    }
  }

  async function remove(): Promise<void> {
    if (!sprint || busy) return;
    busy = true;
    try {
      await deleteSprint(board.id, sprint.id);
      open = false;
      toasts.show({ tone: 'success', message: m.sprintDeleted() });
      ondeleted?.(sprint);
    } catch (error) {
      confirming = false;
      fail(error);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  bind:open
  title={confirming && sprint
    ? m.deleteSprintTitle({ name: sprint.name })
    : sprint
      ? m.editSprint()
      : m.newSprint()}
  size="sm"
  dismissible={!busy}
>
  {#if confirming && sprint}
    <p>{m.deleteSprintText()}</p>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  {:else}
    <form id={formId} class="form" novalidate onsubmit={(event) => void save(event)}>
      <TextField
        label={m.sprintName()}
        bind:value={name}
        required
        maxlength={100}
        autocomplete="off"
        error={errors.name}
      />
      <TextArea
        label={m.sprintGoal()}
        hint={m.sprintGoalHint()}
        bind:value={goal}
        rows={2}
        maxlength={1000}
        error={errors.goal}
      />
      <div class="dates">
        <TextField
          type="date"
          label={m.sprintStarts()}
          bind:value={starts}
          error={errors.starts_on}
        />
        <TextField
          type="date"
          label={m.sprintEnds()}
          bind:value={ends}
          min={starts || undefined}
          error={errors.ends_on}
        />
      </div>
      <p class="hint">{m.sprintDatesHint()}</p>
      {#if !sprint}
        {#if carryFrom}
          <Checkbox label={m.carryFrom({ name: carryFrom.name })} bind:checked={carry} />
        {/if}
        <Checkbox label={m.autoTarget()} bind:checked={auto} />
      {/if}
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    </form>
  {/if}
  {#snippet footer()}
    {#if confirming}
      <Button variant="ghost" disabled={busy} onclick={() => (confirming = false)}
        >{m.cancel()}</Button
      >
      <Button variant="danger" loading={busy} onclick={() => void remove()}
        >{m.deleteSprint()}</Button
      >
    {:else}
      {#if sprint}
        <span class="start">
          <Button variant="ghost" disabled={busy} onclick={() => (confirming = true)}
            >{m.deleteSprint()}</Button
          >
          <Button variant="ghost" disabled={busy} onclick={() => void archive()}
            >{sprint.archived ? m.restoreSprint() : m.archiveSprint()}</Button
          >
        </span>
      {/if}
      <Button variant="ghost" disabled={busy} onclick={() => (open = false)}>{m.cancel()}</Button>
      <Button type="submit" form={formId} variant="primary" loading={busy}
        >{sprint ? m.saveSprint() : m.createSprint()}</Button
      >
    {/if}
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .dates {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: var(--space-3);
  }

  .hint {
    margin-top: calc(-1 * var(--space-2));
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .start {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-inline-end: auto;
  }
</style>
