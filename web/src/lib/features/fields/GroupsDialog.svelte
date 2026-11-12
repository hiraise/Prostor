<script lang="ts">
  // Adding groups to a user set (FLD-040): groups found by name as checkboxes, so a keyboard, a
  // screen reader and a finger work the same. A team shows as "«Project» team".
  import { untrack } from 'svelte';

  import type { Group } from '#lib/api/schema.gen.ts';
  import { findGroups, groupName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';

  interface Props {
    open?: boolean;
    title: string;
    /** Groups listed already: shown, but not offered again. */
    listed?: readonly number[];
    /** Adds the chosen groups; a rejection is shown in the dialog, which stays open. */
    onconfirm: (groups: number[]) => unknown;
  }

  let { open = $bindable(false), title, listed = [], onconfirm }: Props = $props();

  const name = $props.id();
  let text = $state('');
  let found = $state<Group[]>([]);
  let cursor = $state<string | null>(null);
  let searched = $state(false);
  let loading = $state(false);
  let searchProblem = $state<string | null>(null);
  let chosen = $state<number[]>([]);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let request = 0;

  async function search(more: boolean): Promise<void> {
    const mine = ++request;
    loading = true;
    searchProblem = null;
    try {
      const page = await findGroups(text, more ? cursor : null);
      if (mine !== request) return;
      found = more ? [...found, ...page.items] : page.items;
      cursor = page.next_cursor;
      searched = true;
    } catch (error) {
      if (mine === request) searchProblem = refusalText(error);
    } finally {
      if (mine === request) loading = false;
    }
  }

  // Every opening starts afresh.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      text = '';
      found = [];
      cursor = null;
      searched = false;
      chosen = [];
      problem = null;
    });
  });

  // Search a moment after typing stops; the first page at once.
  $effect(() => {
    if (!open) return;
    const typed = text;
    const timer = setTimeout(() => void search(false), typed ? 250 : 0);
    return () => {
      clearTimeout(timer);
    };
  });

  function toggle(id: number, on: boolean): void {
    chosen = on ? [...chosen.filter((other) => other !== id), id] : chosen.filter((x) => x !== id);
  }

  async function confirm(): Promise<void> {
    if (chosen.length === 0) {
      problem = m.chooseGroups();
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
  {#if open}
    <div class="content">
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
      <TextField
        label={m.findGroups()}
        type="search"
        hint={m.findGroupsHint()}
        bind:value={text}
        autocomplete="off"
        spellcheck="false"
      />
      <fieldset class="results" aria-busy={loading}>
        <legend class="legend">{m.groupsToAdd()}</legend>
        {#if searchProblem}
          <p class="note danger">{searchProblem}</p>
        {:else if !searched}
          <p class="note"><Spinner label={common.loading()} /></p>
        {:else if found.length === 0}
          <p class="note">{m.noGroupsFound()}</p>
        {/if}
        <ul class="list">
          {#each found as group (group.id)}
            {@const isListed = listed.includes(group.id)}
            <li>
              <label class="option">
                <input
                  type="checkbox"
                  {name}
                  checked={isListed || chosen.includes(group.id)}
                  disabled={isListed}
                  onchange={(event) => {
                    toggle(group.id, event.currentTarget.checked);
                  }}
                />
                <span class="group-name">{groupName(group)}</span>
                {#if isListed}<span class="note">{m.listedAlready()}</span>{/if}
              </label>
            </li>
          {/each}
        </ul>
        {#if cursor}
          <div>
            <Button variant="ghost" {loading} onclick={() => search(true)}>{m.showMore()}</Button>
          </div>
        {/if}
      </fieldset>
    </div>
  {/if}

  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="primary" loading={busy} onclick={confirm}>{m.addGroups()}</Button>
  {/snippet}
</Dialog>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .results {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  .legend {
    margin-bottom: var(--space-1);
    padding: 0;
    font-weight: var(--weight-medium);
  }

  .list {
    display: flex;
    flex-direction: column;
    max-height: 18rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    list-style: none;
  }

  .list:empty {
    display: none;
  }

  .option {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-height: var(--control-md);
    padding: var(--space-1_5) var(--space-3);
    cursor: pointer;
  }

  .option:hover {
    background: var(--color-hover);
  }

  li + li .option {
    border-top: 1px solid var(--color-border);
  }

  input {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  @media (pointer: coarse) {
    input {
      width: 1.25rem;
      height: 1.25rem;
    }
  }

  .group-name {
    overflow-wrap: anywhere;
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .danger {
    color: var(--color-danger);
  }
</style>
