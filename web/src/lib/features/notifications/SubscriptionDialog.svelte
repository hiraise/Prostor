<script lang="ts">
  // Creating a subscription (NTF-021) and changing its kinds (NTF-024): create picks a tag or a
  // saved search visible to me — the sidebar's type-ahead (a debounced search over the options as
  // a list, never a hover-only combobox) — and the kinds it delivers; edit only reticks the kinds,
  // where an empty list keeps the subscription but delivers nothing (§3.1). Removal lives on the
  // list's row, not here.
  import type { NotificationKind, NotificationSubscription } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import { api } from '#lib/api/client.ts';
  import * as m from './messages.ts';
  import { createSubscription, kindsOf, setSubscriptionKinds } from './subscriptions.ts';

  interface Props {
    open?: boolean;
    /** The subscription being changed; `null` creates a new one. */
    subscription: NotificationSubscription | null;
    /** The subscriptions that already exist, so their sources cannot be picked again. */
    existing: readonly NotificationSubscription[];
    onsaved: () => unknown;
  }

  let { open = $bindable(false), subscription, existing, onsaved }: Props = $props();

  /** One pickable source: a tag or a saved search, each with its own kind list (NTF-022). */
  interface SourceOption {
    source: 'tag' | 'search';
    id: number;
    name: string;
  }

  const TAGS_LIMIT = 20;
  const SEARCHES_LIMIT = 100;

  let query = $state('');
  let found = $state<SourceOption[]>([]);
  let loading = $state(false);
  let problem = $state<string | null>(null);
  let selected = $state<SourceOption | null>(null);
  let kinds = $state<NotificationKind[]>([]);
  let busy = $state(false);

  const editing = $derived(subscription !== null);
  const source = $derived(subscription?.source ?? selected?.source ?? null);
  const allowed = $derived(source === null ? [] : kindsOf(source));

  // Everything starts clean on open: no pick, no error, the kinds the subscription has.
  $effect(() => {
    if (!open) return;
    problem = null;
    busy = false;
    selected = null;
    query = '';
    found = [];
    kinds = subscription ? [...subscription.kinds] : [];
  });

  // Typing replaces the found sources after a pause, as the sidebar's pickers do; the empty
  // query of a fresh open loads the first page at once.
  $effect(() => {
    if (!open || subscription) return;
    const text = query;
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        problem = null;
        findSources(text, controller.signal).then(
          () => {
            if (!controller.signal.aborted) loading = false;
          },
          (failure: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(failure).message;
            loading = false;
          },
        );
      },
      text ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  /** The tags and saved searches visible to me whose name has the typed part. */
  async function findSources(text: string, signal: AbortSignal): Promise<void> {
    const part = text.trim();
    const lower = part.toLowerCase();
    const [tags, searches] = await Promise.all([
      api.get('/tags', { query: { q: part, scope: 'all', limit: TAGS_LIMIT }, signal }),
      api.get('/saved-searches', { query: { scope: 'all', limit: SEARCHES_LIMIT }, signal }),
    ]);
    const matches = (name: string): boolean => lower === '' || name.toLowerCase().includes(lower);
    found = [
      ...tags.items
        .filter((tag) => matches(tag.name))
        .map((tag): SourceOption => ({ source: 'tag', id: tag.id, name: tag.name })),
      ...searches.items
        .filter((search) => matches(search.name))
        .map((search): SourceOption => ({ source: 'search', id: search.id, name: search.name })),
    ];
  }

  function keyOf(option: SourceOption): string {
    return `${option.source}:${String(option.id)}`;
  }

  function taken(option: SourceOption): boolean {
    return existing.some((one) => one.source === option.source && one.source_id === option.id);
  }

  function pick(option: SourceOption, on: boolean): void {
    if (!on || (selected !== null && keyOf(selected) === keyOf(option))) return;
    selected = option;
    // A fresh subscription hears about everything its source may deliver.
    kinds = [...kindsOf(option.source)];
    problem = null;
  }

  function tick(kind: NotificationKind, on: boolean): void {
    const has = kinds.includes(kind);
    const picked = on ? (has ? kinds : [...kinds, kind]) : kinds.filter((one) => one !== kind);
    // Keep the source's own order.
    kinds = allowed.filter((one) => picked.includes(one));
  }

  async function submit(): Promise<void> {
    if (busy) return;
    problem = null;
    if (!editing && (selected === null || kinds.length === 0)) {
      problem = selected === null ? m.pickFirst() : m.pickKind();
      return;
    }
    busy = true;
    try {
      if (subscription) {
        const same =
          subscription.kinds.length === kinds.length &&
          subscription.kinds.every((kind, index) => kinds[index] === kind);
        if (!same) {
          await setSubscriptionKinds(subscription.id, kinds);
          toasts.show({ tone: 'success', message: m.subscriptionSaved() });
        }
      } else if (selected) {
        const created = await createSubscription(selected.source, selected.id, kinds);
        toasts.show({ tone: 'success', message: m.subscribedToast({ name: created.name }) });
      }
      open = false;
      await onsaved();
    } catch (error) {
      problem = describeError(error).message;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  bind:open
  title={editing ? m.editSubscriptionTitle() : m.newSubscriptionTitle()}
  dismissible={!busy}
>
  <div class="body">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#if editing && subscription}
      <p class="source">
        {subscription.name} · {m.sourceWords[subscription.source]?.() ?? subscription.source}
      </p>
    {:else}
      <OptionList
        label={m.pickSource()}
        options={found}
        key={keyOf}
        checked={(option: SourceOption) => selected !== null && keyOf(selected) === keyOf(option)}
        disabled={taken}
        onchange={pick}
        bind:query
        searchLabel={m.findSource()}
        {loading}
      >
        {#snippet option(option: SourceOption)}
          {option.name}
        {/snippet}
        {#snippet secondary(option: SourceOption)}
          {m.sourceWords[option.source]?.() ?? option.source}
          {#if taken(option)}· {m.alreadySubscribed()}{/if}
        {/snippet}
      </OptionList>
    {/if}
    {#if allowed.length > 0}
      <fieldset class="kinds" disabled={busy}>
        <legend>{m.kindsLegend()}</legend>
        {#if editing && kinds.length === 0}<p class="empty">{m.emptyKindsHint()}</p>{/if}
        <div class="ticks">
          {#each allowed as kind (kind)}
            <Checkbox
              label={m.subscriptionKindLabels[kind]?.() ?? kind}
              checked={kinds.includes(kind)}
              onchange={(event: Event & { currentTarget: EventTarget & HTMLInputElement }) =>
                tick(kind, event.currentTarget.checked)}
            />
          {/each}
        </div>
      </fieldset>
    {/if}
  </div>
  {#snippet footer()}
    <Button
      variant="ghost"
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="primary" type="button" loading={busy} onclick={() => void submit()}>
      {editing ? m.save() : m.addSubscription()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .source {
    margin: 0;
    font-weight: var(--weight-medium);
  }

  .kinds {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: none;
  }

  .kinds legend {
    padding: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .ticks {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: var(--space-2) var(--space-6);
  }

  .empty {
    margin: 0;
    color: var(--color-text-muted);
  }
</style>
