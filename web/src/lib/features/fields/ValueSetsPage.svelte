<script lang="ts">
  // The value sets of the instance (FLD-025, FLD-029): by name, with their type and how many
  // projects and fields use them; search by name and filter by type. System administrators create
  // sets; each row opens the set's page.
  import Plus from '@lucide/svelte/icons/plus';
  import SearchX from '@lucide/svelte/icons/search-x';
  import { untrack } from 'svelte';
  import { resolve } from '$app/paths';

  import type { ValueSet } from '#lib/api/schema.gen.ts';
  import { can, canSeeFields } from '#lib/session/index.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import CreateSetDialog from './CreateSetDialog.svelte';
  import { SET_KINDS, ensureValueSets, type SetFilters, type StoredSet } from './fields.ts';
  import * as m from './messages.ts';

  interface Props {
    /** The list for `filters`; `null` when the viewer may not see sets. */
    sets: ListHandle<StoredSet> | null;
    filters?: SetFilters;
    oncreated: (set: ValueSet) => unknown;
    onfilter?: (filters: SetFilters) => void;
    home?: string;
  }

  let { sets, filters = { q: '', kind: '' }, oncreated, onfilter, home }: Props = $props();

  const DEBOUNCE_MS = 250;

  let text = $state(untrack(() => filters.q));
  let kind = $state<string>(untrack(() => filters.kind));
  let list = $state.raw(untrack(() => sets));
  let searching = $state(false);
  let searchError = $state<unknown>(null);
  let creating = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;

  const options = $derived<SelectOption[]>([
    { value: '', label: m.anyType() },
    ...SET_KINDS.map((value) => ({ value, label: m.kindNames[value]() })),
  ]);
  const mayCreate = $derived(can('system.write'));

  function current(): SetFilters {
    return { q: text.trim(), kind: SET_KINDS.find((value) => value === kind) ?? '' };
  }

  async function search(): Promise<void> {
    clearTimeout(timer);
    const wanted = current();
    onfilter?.(wanted);
    const mine = ++request;
    searching = true;
    try {
      const found = await ensureValueSets(wanted);
      if (mine !== request) return;
      list = found;
      searchError = null;
    } catch (error) {
      if (mine === request) searchError = error;
    } finally {
      if (mine === request) searching = false;
    }
  }

  function typed(): void {
    clearTimeout(timer);
    timer = setTimeout(() => void search(), DEBOUNCE_MS);
  }

  $effect(() => () => {
    clearTimeout(timer);
  });

  /** Projects using a set, from the list's summary or a full set's usage. */
  function projectCount(set: StoredSet): number {
    return 'projects' in set ? set.projects : set.used_in.items.length + set.used_in.hidden;
  }

  function fieldCount(set: StoredSet): number {
    return 'fields' in set ? set.fields : set.field_count;
  }

  async function created(set: ValueSet): Promise<void> {
    toasts.show({ tone: 'success', message: m.setCreated() });
    void list?.refresh().catch(() => undefined);
    await oncreated(set);
  }
</script>

<svelte:head><title>{m.setsTitle()} · trackflow</title></svelte:head>

{#if !sets || !canSeeFields()}
  <ForbiddenState {home} />
{:else}
  <PageHeader title={m.setsTitle()}>
    {#snippet actions()}
      {#if mayCreate}
        <Button
          variant="primary"
          icon={Plus}
          onclick={() => {
            creating = true;
          }}
        >
          {m.createSet()}
        </Button>
      {/if}
    {/snippet}
  </PageHeader>

  <div class="page">
    <p class="intro">{m.setsIntro()}</p>

    <search class="filters">
      <TextField
        class="find"
        type="search"
        label={m.findSets()}
        hint={m.findSetsHint()}
        bind:value={text}
        oninput={typed}
        onkeydown={(event: KeyboardEvent) => {
          if (event.key === 'Enter') void search();
        }}
        autocomplete="off"
        maxlength={100}
      />
      <Select
        class="kind"
        label={m.fieldType()}
        {options}
        bind:value={kind}
        onchange={() => void search()}
      />
    </search>

    {#if searchError && !searching}
      <ErrorState error={searchError} level={2} onretry={search} />
    {:else if list}
      {#if list.error && list.ids.length === 0}
        <ErrorState error={list.error} level={2} onretry={() => list?.refresh()} />
      {:else if list.ids.length === 0}
        <EmptyState icon={SearchX} title={m.noSetsMatch()} description={m.noSetsMatchText()} />
      {:else}
        <ul class="list" aria-label={m.setList()} aria-busy={searching || list.refreshing}>
          {#each list.items as set (set.id)}
            {@const projects = projectCount(set)}
            {@const fields = fieldCount(set)}
            <li class="set">
              <a
                class="name"
                href={resolve('/(app)/admin/value-sets/[id]', { id: String(set.id) })}
                aria-describedby="set-{set.id}-details"
              >
                {set.name}
              </a>
              <span id="set-{set.id}-details" class="details">
                {[
                  m.kindNames[set.kind](),
                  projects > 0 ? m.usedInCount({ count: projects }) : m.notUsed(),
                  fields > 0 ? m.defaultSetOf({ count: fields }) : null,
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </li>
          {/each}
        </ul>
        {#if list.hasMore}
          <div>
            <Button loading={list.loadingMore} onclick={() => list?.loadMore()}>
              {m.showMore()}
            </Button>
          </div>
        {/if}
      {/if}
    {/if}
  </div>

  {#if mayCreate}<CreateSetDialog bind:open={creating} oncreated={created} />{/if}
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .intro {
    max-width: 44rem;
    color: var(--color-text-muted);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-3) var(--space-4);
  }

  .filters :global(.find) {
    flex: 1 1 16rem;
    max-width: 24rem;
  }

  .filters :global(.kind) {
    flex: 1 1 12rem;
    max-width: 16rem;
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  /* The name is the link; it covers the whole row, so the row is the target (D-019). */
  .set {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    padding: var(--space-1) var(--space-4) var(--space-2);
  }

  .set + .set {
    border-top: 1px solid var(--color-border);
  }

  .set:first-child {
    border-start-start-radius: var(--radius-lg);
    border-start-end-radius: var(--radius-lg);
  }

  .set:last-child {
    border-end-start-radius: var(--radius-lg);
    border-end-end-radius: var(--radius-lg);
  }

  .set:hover {
    background: var(--color-hover);
  }

  .name {
    display: inline-flex;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    color: inherit;
    font-weight: var(--weight-medium);
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  .name::after {
    content: '';
    position: absolute;
    inset: 0;
  }

  .set:hover .name {
    color: var(--color-link);
    text-decoration: underline;
  }

  .details {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .filters :global(.find),
    .filters :global(.kind) {
      max-width: none;
    }
  }
</style>
