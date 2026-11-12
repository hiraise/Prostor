<script lang="ts">
  // The subscriptions half of the «Уведомления» section (NTF-021–NTF-024; plan step 1.10 §4.2):
  // one row per source with the kinds it delivers, plus create, change and remove. Rows wrap on
  // phones and removal is a visible button, not a hover action (D-019).
  import BellPlus from '@lucide/svelte/icons/bell-plus';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { onMount } from 'svelte';

  import type { NotificationSubscription } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import SubscriptionDialog from './SubscriptionDialog.svelte';
  import { deleteSubscription, listSubscriptions } from './subscriptions.ts';

  let subscriptions = $state<NotificationSubscription[] | null>(null);
  let failure = $state<unknown>(null);
  let creating = $state(false);
  let editingOpen = $state(false);
  let editing = $state<NotificationSubscription | null>(null);
  let removing = $state<NotificationSubscription | null>(null);
  let confirmOpen = $state(false);
  let busy = $state(false);

  async function load(): Promise<void> {
    failure = null;
    try {
      subscriptions = await listSubscriptions();
    } catch (error) {
      failure = error;
    }
  }

  onMount(() => void load());

  /** What a row says the source delivers; a stored subscription may deliver nothing (§3.1). */
  function delivers(subscription: NotificationSubscription): string {
    if (subscription.kinds.length === 0) return m.silentSubscription();
    return subscription.kinds.map((kind) => m.subscriptionKindLabels[kind]?.() ?? kind).join(', ');
  }

  async function confirmRemove(): Promise<void> {
    const subscription = removing;
    if (!subscription) return;
    busy = true;
    try {
      await deleteSubscription(subscription.id);
      confirmOpen = false;
      toasts.show({ tone: 'success', message: m.subscriptionRemoved() });
      await load();
    } catch (error) {
      confirmOpen = false;
      showError(error);
    } finally {
      busy = false;
    }
  }
</script>

<div class="subscriptions">
  <h3>{m.subscriptionsTitle()}</h3>
  <div class="intro">
    <p>{m.subscriptionsIntro()}</p>
    <Button
      icon={BellPlus}
      onclick={() => {
        creating = true;
      }}
    >
      {m.addSubscription()}
    </Button>
  </div>

  {#if failure !== null}
    <ErrorState error={failure} level={3} onretry={() => void load()} />
  {:else if subscriptions !== null && subscriptions.length === 0}
    <EmptyState
      icon={BellPlus}
      title={m.noSubscriptions()}
      description={m.noSubscriptionsText()}
      level={3}
    />
  {:else if subscriptions !== null}
    <ul class="list">
      {#each subscriptions as subscription (subscription.id)}
        <li class="row">
          <div class="text">
            <p class="name">{subscription.name}</p>
            <p class="kinds">
              {m.sourceWords[subscription.source]?.() ?? subscription.source} · {delivers(
                subscription,
              )}
            </p>
          </div>
          <div class="actions">
            <IconButton
              icon={Pencil}
              label={m.editSubscriptionLabel({ name: subscription.name })}
              onclick={() => {
                editing = subscription;
                editingOpen = true;
              }}
            />
            <IconButton
              icon={Trash2}
              label={m.removeSubscriptionLabel({ name: subscription.name })}
              onclick={() => {
                removing = subscription;
                confirmOpen = true;
              }}
            />
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<SubscriptionDialog
  bind:open={creating}
  subscription={null}
  existing={subscriptions ?? []}
  onsaved={() => load()}
/>
<SubscriptionDialog
  bind:open={editingOpen}
  subscription={editing}
  existing={subscriptions ?? []}
  onsaved={() => load()}
/>

<Dialog bind:open={confirmOpen} title={m.removeSubscriptionTitle()} size="sm" dismissible={!busy}>
  <p>{m.removeSubscriptionText({ name: removing?.name ?? '' })}</p>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        confirmOpen = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="danger" loading={busy} onclick={() => void confirmRemove()}>
      {m.removeSubscription()}
    </Button>
  {/snippet}
</Dialog>

<style>
  h3 {
    margin: 0;
    font-size: var(--text-md);
  }

  .subscriptions {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .intro {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3) var(--space-6);
  }

  .intro p {
    flex: 1 1 20rem;
    max-width: 44rem;
    margin: 0;
    color: var(--color-text-muted);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
  }

  .row + .row {
    border-top: 1px solid var(--color-border);
  }

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-0_5);
    min-width: 0;
  }

  .name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .kinds {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    gap: var(--space-1);
  }
</style>
