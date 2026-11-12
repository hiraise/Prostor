<script lang="ts">
  // «Общий доступ…» of a tag or a saved search (QRY-065, ISS-037, D-038): for each kind of access
  // «Только я» or chosen people and groups. Only the owner (or a user administrator) with the
  // share capability opens it; the server checks again.
  import type { SharedWith, SharedWithInput } from '#lib/api/schema.gen.ts';
  import { refusal } from '#lib/features/fields/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import SharedWithPicker from './SharedWithPicker.svelte';
  import { updateSearch, updateTag, type Audience, type Shareable } from './sidebar.ts';

  interface Props {
    open?: boolean;
    item: Shareable | null;
    onsaved: () => unknown;
  }

  let { open = $bindable(false), item, onsaved }: Props = $props();

  type Key = 'view' | 'use' | 'edit';
  interface Entry {
    key: Key;
    legend: string;
    value: Audience;
    restricted: boolean;
    error: string | null;
  }

  const id = $props.id();
  let entries = $state<Entry[]>([]);
  let busy = $state(false);
  let problem = $state<string | null>(null);

  const name = $derived(item ? (item.kind === 'tag' ? item.tag.name : item.search.name) : '');

  function entry(key: Key, legend: string, shared: SharedWith | undefined): Entry {
    const value = { users: [...(shared?.users ?? [])], groups: [...(shared?.groups ?? [])] };
    return {
      key,
      legend,
      value,
      restricted: value.users.length + value.groups.length > 0,
      error: null,
    };
  }

  $effect(() => {
    if (!open || !item) return;
    problem = null;
    if (item.kind === 'tag') {
      const sharing = item.tag.sharing;
      entries = [
        entry('view', m.tagView(), sharing?.view),
        entry('use', m.tagUse(), sharing?.use),
        entry('edit', m.tagEdit(), sharing?.edit),
      ];
    } else {
      const sharing = item.search.sharing;
      entries = [
        entry('view', m.searchView(), sharing?.view),
        entry('edit', m.searchEdit(), sharing?.edit),
      ];
    }
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!item) return;
    problem = null;
    let invalid = false;
    for (const e of entries) {
      e.error =
        e.restricted && e.value.users.length + e.value.groups.length === 0
          ? m.nobodyChosen()
          : null;
      invalid ||= e.error !== null;
    }
    if (invalid) return;
    const sharing: Record<string, SharedWithInput> = {};
    for (const e of entries) {
      sharing[e.key] = {
        users: e.value.users.map((u) => u.id),
        groups: e.value.groups.map((g) => g.id),
      };
    }
    busy = true;
    try {
      if (item.kind === 'tag') await updateTag(item.tag, { sharing });
      else await updateSearch(item.search, { sharing });
      toasts.show({ tone: 'success', message: m.sharingSaved() });
      open = false;
      await onsaved();
    } catch (error) {
      problem = refusal(error, { conflict: m.conflict, paths: [] }).message;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.sharingOf({ name })} dismissible={!busy}>
  <form id="{id}-form" class="form" onsubmit={(event) => void submit(event)}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#each entries as e (e.key)}
      <SharedWithPicker
        legend={e.legend}
        bind:value={e.value}
        bind:restricted={e.restricted}
        error={e.error}
      />
    {/each}
  </form>
  {#snippet footer()}
    <Button
      variant="ghost"
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {m.cancel()}
    </Button>
    <Button variant="primary" type="submit" form="{id}-form" loading={busy}>{m.save()}</Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }
</style>
