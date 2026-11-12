<script lang="ts">
  // Deleting a link type (LNK-021): the dialog counts the links that disappear from issues and
  // says the type can be restored. Problems are reported inside the dialog.
  import type { LinkType } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import { deleteLinkType, linkTypeImpact } from './link-types.ts';
  import * as m from './messages.ts';

  interface Props {
    open?: boolean;
    type: LinkType;
    ondeleted: () => unknown;
  }

  let { open = $bindable(false), type, ondeleted }: Props = $props();

  let busy = $state(false);
  let problem = $state<string | null>(null);
  /** Links the type has; `null` while counting. */
  let links = $state<number | null>(null);

  $effect(() => {
    if (!open) return;
    const id = type.id;
    const controller = new AbortController();
    problem = null;
    links = null;
    linkTypeImpact(id, controller.signal).then(
      (impact) => {
        links = impact.links;
      },
      (error: unknown) => {
        if (!controller.signal.aborted) problem = describeError(error).message;
      },
    );
    return () => {
      controller.abort();
    };
  });

  async function confirm(): Promise<void> {
    busy = true;
    problem = null;
    try {
      await deleteLinkType(type.id);
      open = false;
      toasts.show({ tone: 'success', message: m.deleted() });
      await ondeleted();
    } catch (error) {
      problem = describeError(error).message;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.deleteTitle({ name: type.name })} size="sm" dismissible={!busy}>
  <div class="body">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#if links !== null}
      <p>{m.deleteHides({ count: links })}</p>
    {:else if !problem}
      <p>{m.countingLinks()}</p>
    {/if}
    <p>{m.deleteRestorable()}</p>
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
    <Button variant="danger" loading={busy} disabled={links === null && !problem} onclick={confirm}>
      {m.deleteLinkType()}
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
