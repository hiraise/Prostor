<script lang="ts">
  // The issue popup of a board (BRD-079, BRD-080): the issue page in a large dialog, a sheet on
  // phones. A click, Enter or a double click on a card opens it, F2 opens it in edit mode; Esc,
  // the backdrop and × close it. The page itself carries the corner controls — «Открыть страницей»
  // opens the issue's page in a new tab and keeps the board with its popup in place — so the
  // dialog has no title row of its own (`bare`).
  import type { Issue } from '#lib/api/schema.gen.ts';
  import { IssuePage, openIssue, type IssuePageData } from '#lib/features/issues/index.ts';
  import type { EntityHandle } from '#lib/store/index.ts';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';

  import * as m from '../messages.ts';

  interface Props {
    /** The issue to show and whether to start editing; `null` closes the popup. */
    target: { issue: number; edit: boolean } | null;
    onclose: () => unknown;
  }

  let { target, onclose }: Props = $props();

  type Loaded =
    | { kind: 'loading' }
    | { kind: 'found'; issue: EntityHandle<Issue>; page: IssuePageData }
    | { kind: 'missing' }
    | { kind: 'failed'; error: unknown };

  let loaded = $state<Loaded>({ kind: 'loading' });
  let attempt = $state(0);
  const issue = $derived(loaded.kind === 'found' ? loaded.issue.current : undefined);

  $effect(() => {
    const wanted = target?.issue;
    // A retry asks again.
    if (wanted === undefined || attempt < 0) return;
    let current = true;
    loaded = { kind: 'loading' };
    openIssue(String(wanted)).then(
      (answer) => {
        if (!current) return;
        loaded =
          answer.kind === 'missing'
            ? { kind: 'missing' }
            : { kind: 'found', issue: answer.issue, page: answer.page };
      },
      (error: unknown) => {
        if (current) loaded = { kind: 'failed', error };
      },
    );
    return () => {
      current = false;
    };
  });
</script>

<Dialog
  open={target !== null}
  title={issue ? `${issue.readable_id ?? ''} ${issue.summary}`.trim() : m.issuePopup()}
  hideTitle
  size="xl"
  bare
  onclose={() => {
    if (target !== null) onclose();
  }}
>
  <div class="popup">
    {#if issue && target && loaded.kind === 'found'}
      {#key issue.id}
        <IssuePage
          {issue}
          page={loaded.page}
          embedded
          edit={target.edit}
          onclose={() => onclose()}
        />
      {/key}
    {:else if loaded.kind === 'missing'}
      <EmptyState title={m.issueGone()} level={2} />
    {:else if loaded.kind === 'failed'}
      <ErrorState
        error={loaded.error}
        level={2}
        onretry={() => {
          attempt += 1;
        }}
      />
    {:else}
      <Spinner />
    {/if}
  </div>
</Dialog>

<style>
  .popup {
    display: flex;
    flex-direction: column;
    min-height: 12rem;
  }
</style>
