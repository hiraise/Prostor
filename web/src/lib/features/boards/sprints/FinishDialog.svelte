<script lang="ts">
  // Finishing a sprint by hand (D-013) with what to do with its unresolved issues: leave them, or
  // carry them to another open sprint (BRD-101).
  import { untrack } from 'svelte';

  import type { Sprint } from '#lib/api/schema.gen.ts';
  import { refusal } from '#lib/features/fields/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import Select from '#lib/ui/Select.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { carrySprint, sprintAction } from '../boards.ts';
  import * as m from '../messages.ts';

  interface Props {
    open?: boolean;
    board: { id: number };
    sprint: Sprint;
    /** The board's sprints that are not archived. */
    sprints: readonly Sprint[];
    onfinished: (sprint: Sprint) => unknown;
  }

  let { open = $bindable(false), board, sprint, sprints, onfinished }: Props = $props();

  // Open sprints the issues may go to: newest first.
  const targets = $derived(
    [...sprints].reverse().filter((one) => one.id !== sprint.id && !one.finished && !one.internal),
  );

  let choice = $state('keep');
  let target = $state('');
  let problem = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (!open) return;
    untrack(() => {
      choice = 'keep';
      target = targets[0] ? String(targets[0].id) : '';
      problem = null;
    });
  });

  async function finish(): Promise<void> {
    if (busy) return;
    busy = true;
    problem = null;
    try {
      const finished = await sprintAction(board.id, sprint.id, 'finish');
      const to = targets.find((one) => String(one.id) === target);
      if (choice === 'carry' && to) {
        await carrySprint(board.id, sprint.id, to.id);
        toasts.show({ tone: 'success', message: m.carriedTo({ name: to.name }) });
      } else {
        toasts.show({ tone: 'success', message: m.sprintFinishedToast() });
      }
      open = false;
      onfinished(finished);
    } catch (error) {
      problem = refusal(error, { paths: [] }).message ?? m.sprintActionFailed();
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.finishTitle({ name: sprint.name })} size="sm" dismissible={!busy}>
  <div class="body">
    <p>{m.finishText()}</p>
    <RadioGroup
      legend={m.unresolvedChoice()}
      bind:value={choice}
      options={[
        { value: 'keep', label: m.keepUnresolved() },
        { value: 'carry', label: m.carryUnresolved() },
      ]}
    />
    {#if choice === 'carry'}
      {#if targets.length > 0}
        <Select
          label={m.carryTarget()}
          bind:value={target}
          options={targets.map((one) => ({ value: String(one.id), label: one.name }))}
        />
      {:else}
        <Notice tone="warning">{m.noCarryTarget()}</Notice>
      {/if}
    {/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  </div>
  {#snippet footer()}
    <Button variant="ghost" disabled={busy} onclick={() => (open = false)}>{m.cancel()}</Button>
    <Button
      variant="primary"
      loading={busy}
      disabled={choice === 'carry' && targets.length === 0}
      onclick={() => void finish()}>{m.finishSprint()}</Button
    >
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
