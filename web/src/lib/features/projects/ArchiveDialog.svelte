<script lang="ts">
  // Archiving a project (PRJ-061): what it does — read-only, out of pickers and searches — and
  // that it is fully reversible. Problems are reported inside the dialog.
  import type { Project } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { archiveProject } from './projects.ts';

  interface Props {
    open?: boolean;
    project: Pick<Project, 'id' | 'name'>;
    onarchived?: (project: Project) => unknown;
  }

  let { open = $bindable(false), project, onarchived }: Props = $props();

  const id = $props.id();
  let busy = $state(false);
  let problem = $state<string | null>(null);

  // Every opening starts clean.
  $effect(() => {
    if (open) problem = null;
  });

  async function confirm(): Promise<void> {
    busy = true;
    problem = null;
    try {
      const archived = await archiveProject(project.id);
      open = false;
      toasts.show({ tone: 'success', message: m.projectArchived({ name: archived.name }) });
      await onarchived?.(archived);
    } catch (error) {
      problem = describeError(error).message;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.archiveTitle({ name: project.name })} size="sm" dismissible={!busy}>
  <div class="body">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <h3 id="{id}-effects" class="sr-only">{m.archiveEffects()}</h3>
    <ul aria-labelledby="{id}-effects">
      <li>{m.archiveReadOnly()}</li>
      <li>{m.archiveHidden()}</li>
      <li>{m.archiveReversible()}</li>
    </ul>
  </div>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="primary" loading={busy} onclick={confirm}>{m.archiveProject()}</Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding-left: var(--space-5);
  }
</style>
