<script lang="ts">
  // Adding accounts to a group or removing them from one (ACC-026). Adding offers the groups found
  // by name; removing offers the groups the chosen accounts are in. Built-in groups include
  // everyone by themselves, and project teams change on their project's people page, so neither
  // is offered. One choice as radio buttons, so a keyboard,
  // a screen reader and a finger work the same.
  import { untrack } from 'svelte';

  import type { AccountSummary, Group, GroupRef } from '#lib/api/schema.gen.ts';
  import { accessMessages, findGroups, groupName, personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import { addToGroup, removeFromGroup } from './users.ts';

  interface Props {
    open?: boolean;
    mode: 'add' | 'remove';
    accounts: readonly AccountSummary[];
    ondone: () => unknown;
  }

  let { open = $bindable(false), mode, accounts, ondone }: Props = $props();

  /** A group to offer: a found group, or one the accounts are in. */
  type Choice = Pick<Group, 'id' | 'name' | 'builtin' | 'team_of'> &
    Partial<Pick<Group, 'members'>>;

  const name = $props.id();
  let text = $state('');
  let found = $state<Group[]>([]);
  let cursor = $state<string | null>(null);
  let searched = $state(false);
  let loading = $state(false);
  let searchProblem = $state<string | null>(null);
  let chosen = $state<number | null>(null);
  let missing = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let request = 0;

  const title = $derived.by(() => {
    const [first] = accounts;
    if (accounts.length === 1 && first) {
      return mode === 'add'
        ? m.addOneToGroupTitle({ name: personName(first) })
        : m.removeOneFromGroupTitle({ name: personName(first) });
    }
    return mode === 'add'
      ? m.addManyToGroupTitle({ count: accounts.length })
      : m.removeManyFromGroupTitle({ count: accounts.length });
  });

  /** The groups the accounts are in, each once, by name. */
  const joined = $derived.by((): GroupRef[] => {
    const groups: GroupRef[] = [];
    for (const account of accounts) {
      for (const group of account.groups) {
        if (!groups.some((other) => other.id === group.id)) groups.push(group);
      }
    }
    return groups.sort((a, b) => groupName(a).localeCompare(groupName(b)));
  });

  const choices = $derived.by((): Choice[] => {
    if (mode === 'remove') {
      return joined.filter((group) => group.builtin === null && group.team_of === null);
    }
    // One account: the groups it is in already are not offered.
    const [only] = accounts;
    const skip = accounts.length === 1 && only ? only.groups.map((group) => group.id) : [];
    return found.filter(
      (group) => group.builtin === null && group.team_of === null && !skip.includes(group.id),
    );
  });

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
      chosen = null;
      missing = false;
      problem = null;
    });
  });

  // Adding searches a moment after typing stops.
  $effect(() => {
    if (!open || mode !== 'add') return;
    const typed = text;
    const timer = setTimeout(() => void search(false), typed ? 250 : 0);
    return () => {
      clearTimeout(timer);
    };
  });

  async function confirm(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    const group = choices.find((choice) => choice.id === chosen);
    missing = !group;
    if (!group) {
      announce(m.chooseGroup(), 'assertive');
      return;
    }
    const users = accounts.map((account) => account.id);
    busy = true;
    try {
      const label = groupName(group);
      if (mode === 'add') {
        const added = await addToGroup(group.id, users);
        toasts.show({
          tone: 'success',
          message: m.addedToGroup({ group: label, count: added.length }),
        });
      } else {
        const removed = await removeFromGroup(group.id, users);
        toasts.show({
          tone: 'success',
          message: m.removedFromGroup({ group: label, count: removed.length }),
        });
      }
      open = false;
      await ondone();
    } catch (error) {
      problem = refusalText(error, m.builtinGroupsConflict);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open {title} dismissible={!busy}>
  {#if open}
    <form id="{name}-form" class="form" novalidate onsubmit={confirm}>
      {#if mode === 'add'}
        <TextField
          label={m.findGroup()}
          type="search"
          hint={m.findGroupHint()}
          bind:value={text}
          autocomplete="off"
          spellcheck="false"
        />
      {/if}
      <fieldset class="results" aria-busy={loading}>
        <legend class="legend">{m.group()}</legend>
        {#if searchProblem}
          <p class="note danger">{searchProblem}</p>
        {:else if mode === 'add' && !searched}
          <p class="note"><Spinner /></p>
        {:else if choices.length === 0}
          <p class="note">{mode === 'add' ? m.noGroupsFound() : m.noGroupsToLeave()}</p>
        {/if}
        <ul class="list">
          {#each choices as choice (choice.id)}
            <li>
              <label class="option">
                <input
                  type="radio"
                  {name}
                  value={choice.id}
                  checked={chosen === choice.id}
                  onchange={() => {
                    chosen = choice.id;
                  }}
                />
                <span class="group-name">{groupName(choice)}</span>
                {#if choice.members !== undefined}
                  <span class="members">{m.memberCount({ count: choice.members })}</span>
                {/if}
              </label>
            </li>
          {/each}
        </ul>
        {#if mode === 'add' && cursor}
          <div>
            <Button variant="ghost" {loading} onclick={() => search(true)}>
              {accessMessages.showMore()}
            </Button>
          </div>
        {/if}
      </fieldset>
      {#if missing && chosen === null}<Notice tone="danger">{m.chooseGroup()}</Notice>{/if}
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    </form>
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
    <Button type="submit" form="{name}-form" variant="primary" loading={busy}>
      {mode === 'add' ? m.addToGroup() : m.removeFromGroup()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
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

  .members {
    margin-left: auto;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .danger {
    color: var(--color-danger);
  }
</style>
