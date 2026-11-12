<script lang="ts">
  // Choosing people in a dialog (FLD-040, FLD-049, FLD-055): the shared people search, and a
  // confirm button that hands the choice to the caller. A refusal of the caller's change shows
  // inside the dialog.
  import type { AccountSummary } from '#lib/api/schema.gen.ts';
  import { PeopleSearch } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';

  interface Props {
    open?: boolean;
    title: string;
    /** The legend of the choices. */
    label: string;
    mode?: 'single' | 'multiple';
    /** People who cannot be chosen (listed already). */
    exclude?: readonly number[];
    /** The confirm button. */
    action: string;
    /** Applies the choice; a rejection is shown in the dialog, which stays open. */
    onconfirm: (chosen: AccountSummary[]) => unknown;
  }

  let {
    open = $bindable(false),
    title,
    label,
    mode = 'multiple',
    exclude = [],
    action,
    onconfirm,
  }: Props = $props();

  let chosen = $state<AccountSummary[]>([]);
  let busy = $state(false);
  let problem = $state<string | null>(null);

  // Every opening starts with nobody chosen.
  $effect(() => {
    if (!open) return;
    chosen = [];
    problem = null;
  });

  async function confirm(): Promise<void> {
    if (chosen.length === 0) {
      problem = m.chooseSomeone();
      return;
    }
    busy = true;
    problem = null;
    try {
      await onconfirm(chosen);
      open = false;
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open {title} dismissible={!busy}>
  <div class="content">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <!-- Searches only while open. -->
    {#if open}<PeopleSearch {label} {mode} bind:chosen {exclude} />{/if}
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
    <Button variant="primary" loading={busy} onclick={confirm}>{action}</Button>
  {/snippet}
</Dialog>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
