<script lang="ts">
  // Deleting a project (PRJ-064, PRJ-065): what is lost, that it cannot be undone and runs in the
  // background, and a confirmation by typing the project's name exactly. A mismatch is said on
  // the field rather than by a disabled button, so everyone learns why nothing happened. How many
  // issues go with it is said when the count arrives; without it the warning stays general.
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import type { Project } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { deleteProject, projectIssueCount } from './projects.ts';

  interface Props {
    open?: boolean;
    project: Pick<Project, 'id' | 'name'>;
    ondeleted?: (project: Pick<Project, 'id' | 'name'>) => unknown;
  }

  let { open = $bindable(false), project, ondeleted }: Props = $props();

  const formId = $props.id();
  let typed = $state('');
  let mismatch = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let input: HTMLInputElement | undefined = $state();
  /** Issues of the project the viewer sees; `null` until counted or when counting failed. */
  let issues = $state<number | null>(null);
  const warning = $derived(
    issues !== null && issues > 0
      ? `${m.deleteIssueCount({ count: issues })} ${m.deleteForever()}`
      : m.deleteForever(),
  );

  // Every opening starts from an empty field.
  $effect(() => {
    if (!open) return;
    typed = '';
    mismatch = false;
    problem = null;
  });

  // Counts the issues on every opening; a failure leaves the general warning alone.
  $effect(() => {
    if (!open) return;
    const id = project.id;
    const controller = new AbortController();
    issues = null;
    projectIssueCount(id, controller.signal).then(
      (count) => {
        if (!controller.signal.aborted) issues = count.all;
      },
      () => undefined,
    );
    return () => {
      controller.abort();
    };
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    if (typed !== project.name) {
      mismatch = true;
      input?.focus();
      return;
    }
    mismatch = false;
    busy = true;
    const deleted = { id: project.id, name: project.name };
    try {
      await deleteProject(deleted.id);
      open = false;
      toasts.show({ tone: 'success', message: m.projectDeleting({ name: deleted.name }) });
      await ondeleted?.(deleted);
    } catch (error) {
      problem = describeError(error).message;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.deleteTitle({ name: project.name })} size="sm" dismissible={!busy}>
  <form id={formId} class="body" novalidate onsubmit={submit}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <p>{warning}</p>
    <p>{m.deleteInBackground()}</p>
    <TextField
      label={m.typeName()}
      hint={m.typeNameHint({ name: project.name })}
      error={mismatch ? m.nameMismatch() : null}
      bind:value={typed}
      bind:input
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
      oninput={() => {
        mismatch = false;
      }}
    />
  </form>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button type="submit" form={formId} variant="danger" icon={Trash2} loading={busy}>
      {m.deleteProject()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
