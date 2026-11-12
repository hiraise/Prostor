<script lang="ts">
  // One group (ACC-021…ACC-032, ACC-060): what it is, its members, its roles and, for people who
  // manage groups, its settings and deletion. Sections stack on every width, so the page reads the
  // same on a phone.
  import type { Group } from '#lib/api/schema.gen.ts';
  import { builtinGroups, groupName } from '#lib/features/access/index.ts';
  import type { EntityHandle } from '#lib/store/index.ts';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';

  import Badge from './Badge.svelte';
  import DeleteGroupsDialog from './DeleteGroupsDialog.svelte';
  import GroupMembers from './GroupMembers.svelte';
  import GroupRoles from './GroupRoles.svelte';
  import { canManageGroups, forgetGroups } from './groups.ts';
  import GroupSettings from './GroupSettings.svelte';
  import * as m from './messages.ts';

  interface Props {
    group: EntityHandle<Group>;
    /** The group was deleted; the route leaves for the list. */
    ondeleted: () => unknown;
    /** Where "not found" leads. */
    home?: string;
  }

  let { group: handle, ondeleted, home }: Props = $props();

  let deleting = $state(false);

  const group = $derived(handle.current);
  /** A project team changes only on its project's people page (ACC-033). */
  const manage = $derived(canManageGroups() && group?.team_of == null);

  async function deleted(ids: number[]): Promise<void> {
    await ondeleted();
    forgetGroups(ids);
  }
</script>

{#if group}
  <!-- A new group starts with its own form and member search. -->
  {#key group.id}
    <PageHeader title={groupName(group)} />
    <div class="page">
      {#if group.builtin !== null || group.auto_join || group.description !== ''}
        <div class="summary">
          {#if group.builtin !== null || group.auto_join}
            <p class="badges">
              {#if group.builtin}<Badge tone="info">{m.builtinBadge()}</Badge>{/if}
              {#if group.auto_join && !group.builtin}<Badge>{m.autoJoinBadge()}</Badge>{/if}
            </p>
          {/if}
          {#if group.builtin}<p>{builtinGroups[group.builtin].description()}</p>{/if}
          {#if group.description}<p>{group.description}</p>{/if}
        </div>
      {/if}

      <GroupMembers {group} {manage} />
      <GroupRoles {group} />
      {#if manage}
        <GroupSettings
          {group}
          ondelete={() => {
            deleting = true;
          }}
        />
      {/if}
    </div>

    {#if manage && !group.builtin}
      <DeleteGroupsDialog bind:open={deleting} groups={[group]} ondeleted={deleted} />
    {/if}
  {/key}
{:else}
  <NotFoundState {home} />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    max-width: var(--content-max);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
  }

  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  @media (width < 48rem) {
    .page {
      gap: var(--space-6);
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }
</style>
