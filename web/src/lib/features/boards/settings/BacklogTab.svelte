<script lang="ts">
  // «Бэклог» (BRD-089, BRD-090): which saved search is the board's backlog — one of the searches
  // the person sees (the server checks that it names only board projects), none, or a new one
  // made like a new board's («<доска> · бэклог»). Each choice saves at once.
  import { onMount } from 'svelte';

  import { api } from '#lib/api/client.ts';
  import type { Board, SavedSearch } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select from '#lib/ui/Select.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from '../messages.ts';
  import { saveBoard } from './settings.ts';

  interface Props {
    board: Board;
    busy?: boolean;
    onchanged: () => unknown;
  }

  let { board, busy = $bindable(false), onchanged }: Props = $props();

  /** Searches one listing shows; a person rarely has more. */
  const SEARCHES_MAX = 200;

  let searches = $state<SavedSearch[] | null>(null);
  let loadFailed = $state(false);
  let error = $state<string | null>(null);
  let problem = $state<string | null>(null);
  let saving = $state<'choice' | 'create' | null>(null);

  async function loadSearches(): Promise<void> {
    loadFailed = false;
    try {
      const page = await api.get('/saved-searches', {
        query: { scope: 'all', limit: SEARCHES_MAX },
      });
      searches = page.items;
    } catch {
      loadFailed = true;
    }
  }

  onMount(() => {
    void loadSearches();
  });

  const options = $derived.by(() => {
    const listed = (searches ?? []).map((one) => ({ value: String(one.id), label: one.name }));
    // The current backlog may be a search the listing left out.
    const current = board.backlog;
    if (current !== null && !listed.some((one) => one.value === String(current))) {
      listed.unshift({ value: String(current), label: `#${String(current)}` });
    }
    return [{ value: '', label: m.backlogNoSearch() }, ...listed];
  });
  const currentName = $derived(searches?.find((one) => one.id === board.backlog)?.name ?? null);

  async function save(
    part: 'choice' | 'create',
    patch: Parameters<typeof saveBoard>[1],
  ): Promise<boolean> {
    error = null;
    problem = null;
    saving = part;
    busy = true;
    try {
      const found = await saveBoard(board, patch, ['backlog']);
      if (found) {
        error = found.fields.backlog ?? null;
        problem = error ? null : found.message;
        return false;
      }
      await onchanged();
      return true;
    } finally {
      saving = null;
      busy = false;
    }
  }

  function choose(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    void save('choice', { backlog: value === '' ? null : Number(value) }).then((saved) => {
      if (saved) toasts.show({ tone: 'success', message: m.saved() });
    });
  }

  async function create(): Promise<void> {
    if (await save('create', { new_backlog: true })) {
      toasts.show({ tone: 'success', message: m.backlogCreated() });
      await loadSearches();
    }
  }
</script>

<div class="backlog-tab">
  <p class="about">{m.backlogAbout()}</p>
  {#if currentName}<p>{m.backlogCurrent({ name: currentName })}</p>{/if}
  {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  {#if loadFailed}
    <Notice tone="danger">
      {m.backlogSearchesFailed()}
      <Button variant="secondary" size="sm" onclick={() => void loadSearches()}>{m.retry()}</Button>
    </Notice>
  {:else}
    <Select
      label={m.backlogSearch()}
      {options}
      value={board.backlog === null ? '' : String(board.backlog)}
      disabled={searches === null || busy}
      {error}
      onchange={choose}
    />
  {/if}
  <div>
    <Button size="sm" loading={saving === 'create'} onclick={() => void create()}
      >{m.backlogCreate()}</Button
    >
  </div>
</div>

<style>
  .backlog-tab {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .about {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
