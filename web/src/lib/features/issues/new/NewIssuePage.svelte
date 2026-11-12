<script lang="ts">
  // The new-issue page (phones, and «Открыть на всю страницу»): the same form as the dialog. The
  // address names the draft once it exists, so a reload continues it; leaving the page closes the
  // form as the dialog's × does (ISS-010).
  import { goto } from '$app/navigation';
  import { onDestroy, onMount } from 'svelte';

  import { resolve } from '$app/paths';

  import type { Issue, NewIssueLink, ProjectRef } from '#lib/api/schema.gen.ts';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import { issueHref, projectIssuesHref } from '../issues.ts';
  import * as m from '../messages.ts';
  import { beginDraft, endDraft } from './begin.ts';
  import { DraftSession } from './drafts.svelte.ts';
  import NewIssueForm from './NewIssueForm.svelte';
  import { newIssueHref } from './open.svelte.ts';

  interface Props {
    draft?: number;
    /** The key of the entry point's project. */
    project?: string;
    /** Starts a subtask of this issue (ISS-009). */
    parent?: number;
    /** Starts an issue linked so (ISS-009). */
    link?: NewIssueLink;
  }

  let { draft, project, parent, link }: Props = $props();

  const session = new DraftSession();
  let nowhere = $state(false);
  let finished = false;

  onMount(() => {
    beginDraft(session, { draft, project, parent, link }).then(
      (started) => {
        nowhere = !started;
      },
      (error: unknown) => {
        showError(error);
      },
    );
  });

  // A reload continues the draft.
  $effect(() => {
    const id = session.id;
    if (id === null || id === draft) return;
    // newIssueHref resolves the address.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(newIssueHref({ draft: id }), { replace: true, shallow: true, state: {} });
  });

  onDestroy(() => {
    if (!finished) endDraft(session).catch(showError);
  });

  function finish(created?: Issue, left?: ProjectRef): void {
    finished = true;
    // The hrefs are resolved by issueHref and projectIssuesHref.
    /* eslint-disable svelte/no-navigation-without-resolve */
    if (created) void goto(issueHref(created));
    else if (left) void goto(projectIssuesHref(left));
    else void goto(resolve('/(app)/issues'));
    /* eslint-enable svelte/no-navigation-without-resolve */
  }
</script>

<svelte:head><title>{m.newIssue()} · trackflow</title></svelte:head>

<PageHeader title={m.newIssue()} />
<div class="page">
  {#if nowhere}
    <EmptyState title={m.noCreatableProjects()} level={2} />
  {:else if session.issue}
    <NewIssueForm {session} first={project} onfinish={finish} />
  {:else}
    <Spinner />
  {/if}
</div>

<style>
  .page {
    max-width: var(--content-max);
    padding: var(--space-4) var(--space-6) var(--space-12);
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-12);
    }
  }
</style>
