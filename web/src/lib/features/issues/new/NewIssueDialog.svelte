<script lang="ts">
  // The new-issue form in a large dialog (wide screens). Esc, the backdrop and × close it the way
  // leaving the page does: an empty draft is deleted, any other kept (ISS-010). «Открыть на всю
  // страницу» hands the draft over to the page.
  import { goto } from '$app/navigation';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import { issueHref } from '../issues.ts';
  import * as m from '../messages.ts';
  import { beginDraft, endDraft } from './begin.ts';
  import { DraftSession } from './drafts.svelte.ts';
  import NewIssueForm from './NewIssueForm.svelte';
  import { newIssueDialog, newIssueHref } from './open.svelte.ts';

  let session = $state(new DraftSession());
  let nowhere = $state(false);
  /** Closing must not delete or keep the draft: it was published, discarded or handed over. */
  let finished = false;

  $effect(() => {
    if (!newIssueDialog.open) return;
    const target = newIssueDialog.target;
    const next = new DraftSession();
    session = next;
    nowhere = false;
    finished = false;
    beginDraft(next, target).then(
      (started) => {
        nowhere = !started;
      },
      (error: unknown) => {
        showError(error);
        newIssueDialog.open = false;
      },
    );
  });

  function closed(): void {
    if (finished) return;
    endDraft(session).catch(showError);
  }

  function finish(created?: Issue): void {
    finished = true;
    newIssueDialog.open = false;
    // issueHref resolves the address.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    if (created) void goto(issueHref(created));
  }

  async function expand(): Promise<void> {
    const id = session.id;
    if (id === null) return;
    await session.flush().catch(() => undefined);
    session.release();
    finished = true;
    newIssueDialog.open = false;
    // newIssueHref resolves the address.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    await goto(newIssueHref({ draft: id }));
  }
</script>

<Dialog bind:open={newIssueDialog.open} title={m.newIssue()} size="lg" onclose={closed}>
  {#if nowhere}
    <EmptyState title={m.noCreatableProjects()} />
  {:else if session.issue}
    <NewIssueForm
      {session}
      first={newIssueDialog.target.project}
      onfinish={finish}
      onexpand={expand}
    />
  {:else}
    <Spinner />
  {/if}
</Dialog>
