<script lang="ts">
  // Deleting an issue (ISS-028): a confirmation that names it; afterwards the project's issues
  // with a toast. Problems are said inside the dialog.
  import { goto } from '$app/navigation';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { deleteIssue, forgetIssue } from '../actions.ts';
  import { projectIssuesHref } from '../issues.ts';
  import * as m from '../messages.ts';

  let { open = $bindable(false), issue }: { open?: boolean; issue: Issue } = $props();

  let busy = $state(false);
  let problem = $state<string | null>(null);

  const name = $derived(issue.readable_id ?? m.draft());

  async function confirm(): Promise<void> {
    busy = true;
    problem = null;
    const { id, project } = issue;
    const shown = name;
    try {
      await deleteIssue(id);
      open = false;
      toasts.show({ tone: 'success', message: m.deleted({ id: shown }) });
      // projectIssuesHref resolves the address.
      // eslint-disable-next-line svelte/no-navigation-without-resolve
      await goto(projectIssuesHref(project), { replaceState: true });
      forgetIssue(id);
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.deleteTitle({ id: name })} size="sm" dismissible={!busy}>
  <div class="body">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <p>{m.deleteText({ summary: issue.summary || m.noSummary() })}</p>
  </div>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}>{common.cancel()}</Button
    >
    <Button variant="danger" loading={busy} onclick={confirm}>{m.deleteConfirm()}</Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
