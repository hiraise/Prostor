<script lang="ts">
  // The members of a group (ACC-026): search, "Show more", add and remove. Built-in groups include
  // their accounts by themselves (ACC-021), so they list everyone and offer no changes. Members
  // are references without versions, so the list is kept here, like a picker's results.
  import UserMinus from '@lucide/svelte/icons/user-minus';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import Users from '@lucide/svelte/icons/users';

  import type { Group, UserRef } from '#lib/api/schema.gen.ts';
  import {
    PersonName,
    accessMessages,
    builtinGroups,
    groupName,
    personName,
  } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { resolve } from '$app/paths';

  import { canSeeDirectory, currentUser } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import AddMembersDialog from './AddMembersDialog.svelte';
  import Badge from './Badge.svelte';
  import { findMembers, problemOf, removeMembers, type Problem } from './groups.ts';
  import * as m from './messages.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  let { group, manage }: { group: Group; manage: boolean } = $props();

  const id = $props.id();
  let text = $state('');
  let members = $state<UserRef[]>([]);
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let loadingMore = $state(false);
  let searched = $state(false);
  let failure = $state<unknown>(null);
  let adding = $state(false);
  let removing = $state<UserRef | null>(null);
  let confirmOpen = $state(false);
  let busy = $state(false);
  let removeProblem = $state<Problem | null>(null);
  let run = 0;

  const name = $derived(groupName(group));
  const editable = $derived(manage && !group.builtin);
  const me = $derived(currentUser()?.id);

  async function load(more: boolean): Promise<void> {
    const current = ++run;
    if (more) loadingMore = true;
    else loading = true;
    try {
      const page = await findMembers(group.id, text, more ? cursor : null);
      if (current !== run) return;
      members = more ? [...members, ...page.items] : page.items;
      cursor = page.next_cursor;
      searched = true;
      failure = null;
    } catch (error) {
      if (current === run) failure = error;
    } finally {
      if (current === run) loading = loadingMore = false;
    }
  }

  // Search a moment after typing stops; the first page at once.
  $effect(() => {
    const typed = text;
    const timer = setTimeout(() => void load(false), typed ? 250 : 0);
    return () => {
      clearTimeout(timer);
    };
  });

  function askToRemove(member: UserRef): void {
    removing = member;
    removeProblem = null;
    confirmOpen = true;
  }

  async function confirmRemove(): Promise<void> {
    const member = removing;
    if (!member) return;
    busy = true;
    removeProblem = null;
    try {
      await removeMembers(group.id, [member.id]);
      confirmOpen = false;
      toasts.show({ tone: 'success', message: m.memberRemoved() });
      await load(false);
    } catch (error) {
      removeProblem = problemOf(error, { lastAdministrator: m.noAdministratorLeft });
    } finally {
      busy = false;
    }
  }
</script>

<section class="members" aria-labelledby="{id}-title">
  <div class="head">
    <h2 id="{id}-title">{m.membersTitle()}</h2>
    <span class="count">{m.memberCount({ count: group.members })}</span>
    {#if editable}
      <Button
        class="add"
        icon={UserPlus}
        onclick={() => {
          adding = true;
        }}
      >
        {m.addMembers()}
      </Button>
    {/if}
  </div>

  {#if group.builtin}
    <Notice>
      {builtinGroups[group.builtin].description()}
      {m.automaticMembership()}
    </Notice>
  {/if}

  <TextField
    class="search"
    label={m.findMembers()}
    type="search"
    hint={m.findMembersHint()}
    bind:value={text}
    autocomplete="off"
    autocapitalize="none"
    spellcheck="false"
  />

  {#if failure && members.length === 0}
    <ErrorState error={failure} level={3} onretry={() => load(false)} />
  {:else if !searched}
    <p class="note"><Spinner label={common.loading()} /></p>
  {:else if members.length === 0 && !text.trim()}
    <EmptyState
      icon={Users}
      title={m.noMembers()}
      description={editable ? m.noMembersText() : undefined}
      level={3}
    />
  {:else if members.length === 0}
    <p class="note">{m.nobodyFound()}</p>
  {:else}
    <ul class="list" aria-label={m.memberList({ name })} aria-busy={loading}>
      {#each members as member (member.id)}
        <li class="member">
          <span class="person">
            <PersonName
              person={member}
              withUsername
              href={member.kind === 'person' && canSeeDirectory()
                ? resolve('/(app)/users/[id]', { id: String(member.id) })
                : undefined}
            />
            {#if member.id === me}<Badge>{accessMessages.youBadge()}</Badge>{/if}
            {#if member.disabled}<Badge>{accessMessages.disabledBadge()}</Badge>{/if}
          </span>
          {#if editable}
            <IconButton
              icon={UserMinus}
              label={m.removeMemberLabel({ name: personName(member) })}
              onclick={() => {
                askToRemove(member);
              }}
            />
          {/if}
        </li>
      {/each}
    </ul>
    {#if cursor}
      <div>
        <Button loading={loadingMore} onclick={() => load(true)}>{m.showMore()}</Button>
      </div>
    {/if}
  {/if}
</section>

{#if editable}
  <AddMembersDialog
    bind:open={adding}
    {group}
    members={members.map((member) => member.id)}
    onadded={() => load(false)}
  />

  <Dialog bind:open={confirmOpen} title={m.removeMemberTitle()} size="sm" dismissible={!busy}>
    <div class="confirm">
      {#if removeProblem}<ProblemNotice problem={removeProblem} />{/if}
      <p>{m.removeMemberText({ name: removing ? personName(removing) : '', group: name })}</p>
    </div>
    {#snippet footer()}
      <Button
        disabled={busy}
        onclick={() => {
          confirmOpen = false;
        }}
      >
        {common.cancel()}
      </Button>
      <Button variant="danger" loading={busy} onclick={confirmRemove}>{m.removeMember()}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .members {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .head :global(.add) {
    margin-left: auto;
  }

  .members :global(.search) {
    max-width: 24rem;
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .member {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: calc(var(--control-md) + var(--space-2));
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-4);
  }

  .member + .member {
    border-top: 1px solid var(--color-border);
  }

  .person {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    min-width: 0;
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
