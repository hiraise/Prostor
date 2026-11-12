<script lang="ts">
  // One role (ACC-034, ACC-044, ACC-046, ACC-050, ACC-052, ACC-054, ACC-057): its details, its
  // permissions and who holds it. Predefined roles and viewers without `system.write` get a
  // read-only view; custom roles can be renamed, edited and deleted.
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import type { PermissionInfo, Role } from '#lib/api/schema.gen.ts';
  import { permissions, predefinedRoles, roleName } from '#lib/features/access/index.ts';
  import * as states from '#lib/i18n/messages/states.ts';
  import { can, canSeeRoles } from '#lib/session/index.ts';
  import type { EntityHandle, ListHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';

  import DeleteRoleDialog from './DeleteRoleDialog.svelte';
  import * as m from './messages.ts';
  import PermissionEditor from './PermissionEditor.svelte';
  import RoleDetailsForm from './RoleDetailsForm.svelte';
  import RoleHolders from './RoleHolders.svelte';
  import type { RoleAssignment } from './roles.ts';

  interface Props {
    /** `null` when there is no such role or the viewer may not see it. */
    role: EntityHandle<Role> | null;
    catalog: readonly PermissionInfo[];
    holders: ListHandle<RoleAssignment> | null;
    /** The role was deleted: go back to the list. */
    ondeleted: () => unknown;
    /** Where "not found" offers to go. */
    home?: string;
  }

  let { role, catalog, holders, ondeleted, home }: Props = $props();

  let deleting = $state(false);

  const allowed = $derived(canSeeRoles());
  const current = $derived(role?.current);
  const mayChange = $derived(can('system.write'));
  const editable = $derived(current?.builtin === null && mayChange);
  const title = $derived(
    !allowed ? m.rolesTitle() : current ? roleName(current) : states.notFoundTitle(),
  );
</script>

<svelte:head><title>{title} · trackflow</title></svelte:head>

{#if !allowed}
  <ForbiddenState {home} />
{:else if !current}
  <NotFoundState {home} />
{:else}
  <PageHeader title={roleName(current)}>
    {#snippet actions()}
      {#if editable && holders}
        <Button
          icon={Trash2}
          onclick={() => {
            deleting = true;
          }}
        >
          {m.deleteRole()}
        </Button>
      {/if}
    {/snippet}
  </PageHeader>

  <div class="page">
    {#if current.builtin}
      <div class="about">
        <p class="description">{predefinedRoles[current.builtin].description()}</p>
        <Notice>{m.predefinedNotice()}</Notice>
      </div>
    {:else if !mayChange}
      <div class="about">
        {#if current.description}<p class="description">{current.description}</p>{/if}
        <Notice>{m.readOnlyNotice({ permission: permissions['system.write'].name() })}</Notice>
      </div>
    {/if}

    {#if editable}
      <section class="section" aria-labelledby="role-details">
        <h2 id="role-details">{m.details()}</h2>
        <RoleDetailsForm role={current} />
      </section>
    {/if}

    <PermissionEditor role={current} {catalog} {editable} />

    {#if holders}<RoleHolders {holders} />{/if}
  </div>

  {#if editable && holders}
    <DeleteRoleDialog bind:open={deleting} role={current} {holders} {ondeleted} />
  {/if}
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .about {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: 44rem;
  }

  .description {
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  h2 {
    font-size: var(--text-lg);
  }

  @media (width < 48rem) {
    .page {
      gap: var(--space-6);
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }
</style>
