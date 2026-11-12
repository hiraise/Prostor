<script lang="ts">
  // Puts an issue on a board (ISS-020, BRD-033): a board that includes its project, then a sprint
  // of that board — the current one unless another is chosen. Boards without sprints ask for no
  // sprint. Boards already holding the issue are left out.
  import { untrack } from 'svelte';

  import type { BoardSummary, IssueBoard, Sprint } from '#lib/api/schema.gen.ts';
  import { refusal } from '#lib/features/fields/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select from '#lib/ui/Select.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';

  import { addIssueBoard, findBoards, listSprints } from '../boards.ts';
  import * as m from '../messages.ts';

  interface Props {
    open?: boolean;
    issue: number;
    project: number;
    /** Boards holding the issue now: not offered. */
    holding: readonly number[];
    onadded: (boards: IssueBoard[], board: string) => unknown;
  }

  let { open = $bindable(false), issue, project, holding, onadded }: Props = $props();

  let boards = $state<BoardSummary[] | null>(null);
  let sprints = $state<Sprint[] | null>(null);
  let board = $state('');
  let sprint = $state('');
  let problem = $state<string | null>(null);
  let busy = $state(false);

  const choices = $derived(boards?.filter((one) => !holding.includes(one.id)) ?? []);
  // Open sprints newest first; a sprintless board has only its hidden one.
  const sprintChoices = $derived(
    sprints ? [...sprints].reverse().filter((one) => !one.archived && !one.internal) : [],
  );

  $effect(() => {
    if (!open) return;
    const controller = new AbortController();
    untrack(() => {
      boards = null;
      board = '';
      problem = null;
      findBoards({ project }, null, controller.signal).then(
        (list) => {
          boards = list.items;
          board = choices[0] ? String(choices[0].id) : '';
        },
        (error: unknown) => {
          if (controller.signal.aborted) return;
          boards = [];
          problem = refusal(error, { paths: [] }).message ?? m.issueBoardsFailed();
        },
      );
    });
    return () => controller.abort();
  });

  $effect(() => {
    const chosen = Number(board);
    sprints = null;
    sprint = '';
    if (!open || !chosen) return;
    const controller = new AbortController();
    listSprints(chosen, controller.signal).then(
      (list) => (sprints = list),
      () => {
        if (!controller.signal.aborted) sprints = [];
      },
    );
    return () => controller.abort();
  });

  async function add(): Promise<void> {
    const chosen = choices.find((one) => String(one.id) === board);
    if (busy || !chosen) return;
    busy = true;
    problem = null;
    try {
      const answer = await addIssueBoard(issue, chosen.id, sprint ? Number(sprint) : undefined);
      open = false;
      onadded(answer, chosen.name);
    } catch (error) {
      problem = refusal(error, { paths: [] }).message ?? m.addFailed();
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.addToBoard()} size="sm" dismissible={!busy}>
  <div class="body">
    {#if boards === null}
      <div class="loading"><Spinner label={common.loading()} /></div>
    {:else if choices.length === 0}
      {#if !problem}<Notice tone="info">{m.noBoardsForIssue()}</Notice>{/if}
    {:else}
      <Select
        label={m.boardChoice()}
        bind:value={board}
        options={choices.map((one) => ({ value: String(one.id), label: one.name }))}
      />
      {#if sprintChoices.length > 0}
        <Select
          label={m.sprintChoice()}
          bind:value={sprint}
          options={[
            { value: '', label: m.currentSprintChoice() },
            ...sprintChoices.map((one) => ({ value: String(one.id), label: one.name })),
          ]}
        />
      {/if}
    {/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  </div>
  {#snippet footer()}
    <Button variant="ghost" disabled={busy} onclick={() => (open = false)}>{m.cancel()}</Button>
    <Button
      variant="primary"
      loading={busy}
      disabled={choices.length === 0 || (board !== '' && sprints === null)}
      onclick={() => void add()}>{m.add()}</Button
    >
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: var(--space-3);
  }
</style>
