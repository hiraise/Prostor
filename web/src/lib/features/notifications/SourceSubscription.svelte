<script lang="ts">
  // The subscription section of the tag and saved-search edit dialogs (NTF-025; plan step 1.10
  // §4.2): the kinds this source delivers to me. A tick applies at once — it creates my
  // subscription on the source when none exists, or changes its kinds; an unticked-everything
  // list keeps the subscription stored but silent (§3.1). The dialogs' own save does not touch it.
  import type {
    NotificationKind,
    NotificationSubscription,
    SubscriptionSourceKind,
  } from '#lib/api/schema.gen.ts';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import {
    createSubscription,
    findSubscription,
    kindsOf,
    listSubscriptions,
    setSubscriptionKinds,
  } from './subscriptions.ts';

  interface Props {
    /** Only while the dialog around is open does the section load and show. */
    open: boolean;
    source: SubscriptionSourceKind;
    sourceId: number;
  }

  let { open, source, sourceId }: Props = $props();

  let existing = $state<NotificationSubscription | null>(null);
  let kinds = $state<NotificationKind[]>([]);
  let loading = $state(false);
  let failed = $state(false);
  let busy = $state(false);

  const allowed = $derived(kindsOf(source));

  $effect(() => {
    if (!open) return;
    loading = true;
    failed = false;
    listSubscriptions().then(
      (subscriptions) => {
        existing = findSubscription(subscriptions, source, sourceId);
        kinds = existing ? [...existing.kinds] : [];
        loading = false;
      },
      () => {
        failed = true;
        loading = false;
      },
    );
  });

  async function tick(kind: NotificationKind, on: boolean): Promise<void> {
    if (loading || busy) return;
    const before = kinds;
    const has = kinds.includes(kind);
    const picked = on ? (has ? kinds : [...kinds, kind]) : kinds.filter((one) => one !== kind);
    kinds = allowed.filter((one) => picked.includes(one));
    busy = true;
    try {
      // Some kinds: create the subscription on first use, change the existing one otherwise.
      if (kinds.length > 0) {
        const saved =
          existing === null
            ? await createSubscription(source, sourceId, kinds)
            : await setSubscriptionKinds(existing.id, kinds);
        existing = saved;
      } else if (existing !== null) {
        // None ticked any more: the row stays, but delivers nothing (§3.1).
        await setSubscriptionKinds(existing.id, []);
      }
    } catch (error) {
      kinds = before;
      showError(error);
    } finally {
      busy = false;
    }
  }
</script>

{#if open}
  <fieldset class="section" disabled={busy}>
    <legend>{m.sectionLegend()}</legend>
    <p class="hint">{source === 'tag' ? m.sectionAboutTag() : m.sectionAboutSearch()}</p>
    {#if loading}
      <p class="note"><Spinner /></p>
    {:else if failed}
      <Notice tone="danger">{m.loadingFailed()}</Notice>
    {:else}
      <div class="ticks">
        {#each allowed as kind (kind)}
          <Checkbox
            label={m.subscriptionKindLabels[kind]?.() ?? kind}
            checked={kinds.includes(kind)}
            onchange={(event: Event & { currentTarget: EventTarget & HTMLInputElement }) =>
              void tick(kind, event.currentTarget.checked)}
          />
        {/each}
      </div>
    {/if}
  </fieldset>
{/if}

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3) 0 0;
    border: none;
    border-top: 1px solid var(--color-border);
  }

  .section legend {
    padding: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .hint {
    margin: 0;
    color: var(--color-text-muted);
  }

  .ticks {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: var(--space-2) var(--space-6);
  }

  .note {
    margin: 0;
    color: var(--color-text-muted);
  }
</style>
