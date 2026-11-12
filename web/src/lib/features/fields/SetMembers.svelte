<script lang="ts">
  // The people and groups of a user set (FLD-039, FLD-040): the field offers them and every member
  // of the groups. Both lists are sorted by name; there is no manual order. Removing one offers
  // Undo, since adding it back is the same change.
  import UserMinus from '@lucide/svelte/icons/user-minus';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import UsersRound from '@lucide/svelte/icons/users-round';

  import type { AccountSummary, GroupRef, UserRef, ValueSet } from '#lib/api/schema.gen.ts';
  import { PersonName, groupName, personName } from '#lib/features/access/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import { addSetMembers, removeSetMembers } from './fields.ts';
  import GroupsDialog from './GroupsDialog.svelte';
  import * as m from './messages.ts';
  import PeopleDialog from './PeopleDialog.svelte';

  interface Props {
    set: ValueSet;
    /** Level of the headings of the two lists. */
    level?: 3 | 4;
    /** May add and remove people and groups. */
    editable: boolean;
    onchanged?: () => unknown;
  }

  let { set, level = 3, editable, onchanged }: Props = $props();

  const id = $props.id();
  let addingPeople = $state(false);
  let addingGroups = $state(false);
  let removing = $state<string | null>(null);

  const byName = (a: string, b: string): number => a.localeCompare(b);
  const people = $derived([...set.users].sort((a, b) => byName(personName(a), personName(b))));
  const groups = $derived([...set.groups].sort((a, b) => byName(groupName(a), groupName(b))));

  async function addPeople(chosen: AccountSummary[]): Promise<void> {
    const before = new Set(set.users.map((user) => user.id));
    const saved = await addSetMembers(set.id, { users: chosen.map((person) => person.id) });
    const count = saved.users.filter((user) => !before.has(user.id)).length;
    toasts.show({ tone: 'success', message: m.peopleAdded({ count }) });
    await onchanged?.();
  }

  async function addGroups(chosen: number[]): Promise<void> {
    const before = new Set(set.groups.map((group) => group.id));
    const saved = await addSetMembers(set.id, { groups: chosen });
    const count = saved.groups.filter((group) => !before.has(group.id)).length;
    toasts.show({ tone: 'success', message: m.groupsAdded({ count }) });
    await onchanged?.();
  }

  async function remove(
    member: { kind: 'user'; ref: UserRef } | { kind: 'group'; ref: GroupRef },
  ): Promise<void> {
    const key = `${member.kind}:${String(member.ref.id)}`;
    const change =
      member.kind === 'user' ? { users: [member.ref.id] } : { groups: [member.ref.id] };
    const label = member.kind === 'user' ? personName(member.ref) : groupName(member.ref);
    removing = key;
    try {
      await removeSetMembers(set.id, change);
      toasts.show({
        tone: 'success',
        message: m.memberRemoved({ name: label }),
        undo: async () => {
          try {
            await addSetMembers(set.id, change);
            await onchanged?.();
          } catch (error) {
            showError(error);
          }
        },
      });
      await onchanged?.();
    } catch (error) {
      showError(error);
    } finally {
      removing = null;
    }
  }
</script>

<div class="members">
  <p class="intro">{m.membersIntro()}</p>

  <section class="part" aria-labelledby="{id}-people">
    <div class="head">
      <svelte:element this={`h${String(level)}`} id="{id}-people" class="part-title">
        {m.people()}
      </svelte:element>
      {#if editable}
        <Button
          icon={UserPlus}
          onclick={() => {
            addingPeople = true;
          }}
        >
          {m.addPeople()}
        </Button>
      {/if}
    </div>
    {#if people.length === 0}
      <p class="note">{m.noPeopleListed()}</p>
    {:else}
      <ul class="list" aria-label={m.peopleList({ name: set.name })}>
        {#each people as person (person.id)}
          <li class="member">
            <span class="who"><PersonName {person} withUsername /></span>
            {#if editable}
              <IconButton
                icon={UserMinus}
                label={m.removeMember({ name: personName(person) })}
                disabled={removing === `user:${String(person.id)}`}
                onclick={() => remove({ kind: 'user', ref: person })}
              />
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="part" aria-labelledby="{id}-groups">
    <div class="head">
      <svelte:element this={`h${String(level)}`} id="{id}-groups" class="part-title">
        {m.groups()}
      </svelte:element>
      {#if editable}
        <Button
          icon={UsersRound}
          onclick={() => {
            addingGroups = true;
          }}
        >
          {m.addGroups()}
        </Button>
      {/if}
    </div>
    {#if groups.length === 0}
      <p class="note">{m.noGroupsListed()}</p>
    {:else}
      <ul class="list" aria-label={m.groupList({ name: set.name })}>
        {#each groups as group (group.id)}
          <li class="member">
            <span class="who group">{groupName(group)}</span>
            {#if editable}
              <IconButton
                icon={UserMinus}
                label={m.removeMember({ name: groupName(group) })}
                disabled={removing === `group:${String(group.id)}`}
                onclick={() => remove({ kind: 'group', ref: group })}
              />
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

{#if editable}
  <PeopleDialog
    bind:open={addingPeople}
    title={m.addPeopleTitle({ name: set.name })}
    label={m.peopleToAdd()}
    exclude={set.users.map((user) => user.id)}
    action={m.addPeople()}
    onconfirm={addPeople}
  />
  <GroupsDialog
    bind:open={addingGroups}
    title={m.addGroupsTitle({ name: set.name })}
    listed={set.groups.map((group) => group.id)}
    onconfirm={addGroups}
  />
{/if}

<style>
  .members {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .intro,
  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .part {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .part-title {
    font-size: var(--text-md);
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
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-3);
  }

  .member + .member {
    border-top: 1px solid var(--color-border);
  }

  .who {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
