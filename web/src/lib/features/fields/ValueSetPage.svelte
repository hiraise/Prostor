<script lang="ts">
  // One value set (FLD-029…FLD-049): its values or members in the set editor, renaming for those
  // who may change it, and deleting it once nothing uses it (system administrators).
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import * as common from '#lib/i18n/messages/common.ts';
  import { can, canSeeFields } from '#lib/session/index.ts';
  import type { EntityHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { deleteValueSet, forgetValueSet, isFullSet, useCount, type StoredSet } from './fields.ts';
  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import RenameSetDialog from './RenameSetDialog.svelte';
  import ValueSetEditor from './ValueSetEditor.svelte';

  interface Props {
    /** The set; `null` when it does not exist. */
    set: EntityHandle<StoredSet> | null;
    ondeleted: () => unknown;
    home?: string;
  }

  let { set: handle, ondeleted, home }: Props = $props();

  let renaming = $state(false);
  let deleteOpen = $state(false);
  let deleteBusy = $state(false);
  let deleteProblem = $state<string | null>(null);

  const set = $derived.by(() => {
    const row = handle?.current;
    return isFullSet(row) ? row : null;
  });
  const unused = $derived(set !== null && useCount(set) === 0 && set.field_count === 0);

  async function confirmDelete(): Promise<void> {
    if (!set) return;
    const id = set.id;
    deleteBusy = true;
    deleteProblem = null;
    try {
      await deleteValueSet(id);
      deleteOpen = false;
      toasts.show({ tone: 'success', message: m.setDeleted() });
      await ondeleted();
      forgetValueSet(id);
    } catch (error) {
      deleteProblem = refusalText(error, m.setInUse);
    } finally {
      deleteBusy = false;
    }
  }
</script>

<svelte:head><title>{set?.name ?? m.setsTitle()} · TaskMaster</title></svelte:head>

{#if !canSeeFields()}
  <ForbiddenState {home} />
{:else if !set}
  <NotFoundState {home} />
{:else}
  <PageHeader title={set.name}>
    {#snippet actions()}
      {#if set.editable}
        <Button
          icon={Pencil}
          onclick={() => {
            renaming = true;
          }}
        >
          {m.rename()}
        </Button>
      {/if}
      {#if can('system.write') && unused}
        <Button
          variant="danger"
          icon={Trash2}
          onclick={() => {
            deleteProblem = null;
            deleteOpen = true;
          }}
        >
          {m.deleteSet()}
        </Button>
      {/if}
    {/snippet}
  </PageHeader>

  <div class="page">
    <p class="kind">{m.fieldType()}: {m.kindNames[set.kind]()}</p>
    {#if can('system.write') && !unused}<p class="note">{m.setInUse()}</p>{/if}
    <ValueSetEditor {set} level={2} />
  </div>

  {#if set.editable}<RenameSetDialog bind:open={renaming} {set} />{/if}

  <Dialog
    bind:open={deleteOpen}
    title={m.deleteSetTitle({ name: set.name })}
    size="sm"
    dismissible={!deleteBusy}
  >
    <div class="confirm">
      {#if deleteProblem}<Notice tone="danger">{deleteProblem}</Notice>{/if}
      <p>{m.deleteSetText()}</p>
    </div>
    {#snippet footer()}
      <Button
        disabled={deleteBusy}
        onclick={() => {
          deleteOpen = false;
        }}
      >
        {common.cancel()}
      </Button>
      <Button variant="danger" loading={deleteBusy} onclick={confirmDelete}>
        {m.deleteSet()}
      </Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 48rem;
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .kind,
  .note {
    color: var(--color-text-muted);
  }

  .note {
    font-size: var(--text-sm);
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }
</style>
