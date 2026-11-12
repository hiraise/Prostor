<script lang="ts">
  // The groups of a person (ACC-018, ACC-026): the groups they were added to, and the built-in
  // ones every account is in by itself (Everyone; All members unless it is the guest). People who
  // manage groups add the person to a group or remove them from one, after a confirmation.
  // Project teams are shown with the projects, not here: they change on the project's people page.
  import Plus from '@lucide/svelte/icons/plus';
  import X from '@lucide/svelte/icons/x';
  import { resolve } from '$app/paths';

  import type { GroupRef } from '#lib/api/schema.gen.ts';
  import { builtinGroups, groupName, personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can, canAnywhere, canSeeGroups } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import GroupDialog from './GroupDialog.svelte';
  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import { removeFromGroup, type DirectoryAccount } from './users.ts';

  interface Props {
    account: DirectoryAccount;
    /** Called after the person joined or left a group: their roles may have changed. */
    onchanged: () => unknown;
  }

  let { account, onchanged }: Props = $props();

  const headingId = $props.id();
  const groups = $derived(account.groups.filter((group) => group.team_of === null));
  const mayManage = $derived(
    !account.guest && (can('system.write') || canAnywhere('project.update')),
  );
  const builtin = $derived([
    builtinGroups.everyone.name(),
    ...(account.guest ? [] : [builtinGroups.all_members.name()]),
  ]);

  let adding = $state(false);
  let leaving = $state<GroupRef | null>(null);
  let leaveOpen = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);

  function askToLeave(group: GroupRef): void {
    leaving = group;
    problem = null;
    leaveOpen = true;
  }

  async function leave(): Promise<void> {
    const group = leaving;
    if (!group) return;
    busy = true;
    problem = null;
    try {
      await removeFromGroup(group.id, [account.id]);
      leaveOpen = false;
      toasts.show({
        tone: 'success',
        message: m.removedFromGroup({ group: groupName(group), count: 1 }),
      });
      await onchanged();
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

<section class="section" aria-labelledby={headingId}>
  <div class="head">
    <h2 id={headingId}>{m.groupsTitle()}</h2>
    {#if mayManage}
      <Button icon={Plus} onclick={() => (adding = true)}>{m.addToGroup()}</Button>
    {/if}
  </div>
  <ul class="groups">
    {#each groups as group (group.id)}
      <li class="group">
        <span class="name">
          {#if canSeeGroups()}
            <a href={resolve('/(app)/admin/groups/[id]', { id: String(group.id) })}
              >{groupName(group)}</a
            >
          {:else}
            {groupName(group)}
          {/if}
        </span>
        {#if mayManage}
          <IconButton
            icon={X}
            label={m.removeFromGroupLabel({ group: groupName(group) })}
            onclick={() => {
              askToLeave(group);
            }}
          />
        {/if}
      </li>
    {:else}
      <li class="group empty">{m.noOwnGroups()}</li>
    {/each}
    {#each builtin as name (name)}
      <li class="group">
        <span class="name">{name}</span>
        <span class="note">{m.builtinGroup()}</span>
      </li>
    {/each}
  </ul>
</section>

<GroupDialog bind:open={adding} mode="add" accounts={[account]} ondone={onchanged} />

<Dialog
  bind:open={leaveOpen}
  title={m.leaveGroupTitle({ name: personName(account), group: leaving ? groupName(leaving) : '' })}
  size="sm"
  dismissible={!busy}
>
  <div class="body">
    <p>{m.leaveGroupText()}</p>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  </div>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        leaveOpen = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="danger" loading={busy} onclick={leave}>{m.removeFromGroup()}</Button>
  {/snippet}
</Dialog>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .groups {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .group {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: calc(var(--control-md) + var(--space-2));
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-4);
  }

  .group + .group {
    border-top: 1px solid var(--color-border);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .name a {
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    min-width: var(--control-md);
    color: var(--color-link);
  }

  .note,
  .empty {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
