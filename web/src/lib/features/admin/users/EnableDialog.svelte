<script lang="ts">
  // Enabling disabled accounts again, after a confirmation (ACC-005).
  import type { AccountSummary } from '#lib/api/schema.gen.ts';
  import { personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import { enableAccounts } from './users.ts';

  interface Props {
    open?: boolean;
    accounts: readonly AccountSummary[];
    ondone: (ids: number[]) => unknown;
  }

  let { open = $bindable(false), accounts, ondone }: Props = $props();

  let busy = $state(false);
  let problem = $state<string | null>(null);

  const title = $derived.by(() => {
    const [first] = accounts;
    return accounts.length === 1 && first
      ? m.enableOneTitle({ name: personName(first) })
      : m.enableManyTitle({ count: accounts.length });
  });

  $effect(() => {
    if (open) problem = null;
  });

  async function confirm(): Promise<void> {
    problem = null;
    const ids = accounts.map((account) => account.id);
    busy = true;
    try {
      await enableAccounts(ids);
      open = false;
      toasts.show({ tone: 'success', message: m.enabledDone({ count: ids.length }) });
      await ondone(ids);
    } catch (error) {
      problem = refusalText(error, m.enableConflict);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open {title} size="sm" dismissible={!busy}>
  <div class="body">
    <p>{m.enableText()}</p>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
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
    <Button variant="primary" loading={busy} onclick={confirm}>
      {m.enableConfirm({ count: accounts.length })}
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
