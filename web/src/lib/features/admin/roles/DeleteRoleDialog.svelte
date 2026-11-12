<script lang="ts">
  // Deleting a custom role (ACC-057): the dialog names who loses it and where, then the role and
  // all its assignments go. Problems are reported inside the dialog.
  import { ApiError } from '#lib/api/client.ts';
  import type { Role } from '#lib/api/schema.gen.ts';
  import { groupName, personName, predefinedRoles } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { refreshAccess } from '#lib/session/index.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { deleteRole, forgetRole, type RoleAssignment } from './roles.ts';

  interface Props {
    open?: boolean;
    role: Role;
    holders: ListHandle<RoleAssignment>;
    ondeleted: () => unknown;
  }

  let { open = $bindable(false), role, holders, ondeleted }: Props = $props();

  /** Holders named in the dialog; the rest are counted. */
  const SHOWN = 10;

  let busy = $state(false);
  let problem = $state<string | null>(null);

  const listed = $derived(holders.items.slice(0, SHOWN));
  const more = $derived(Math.max(role.assignments - listed.length, 0));

  $effect(() => {
    if (open) problem = null;
  });

  function holderName(assignment: RoleAssignment): string {
    return assignment.holder.kind === 'user'
      ? personName(assignment.holder.user)
      : m.groupHolder({ name: groupName(assignment.holder.group) });
  }

  async function confirm(): Promise<void> {
    busy = true;
    problem = null;
    const held = role.assignments > 0;
    try {
      await deleteRole(role.id);
      open = false;
      toasts.show({ tone: 'success', message: m.roleDeleted() });
      // The signed-in person may have held it.
      if (held) void refreshAccess().catch(() => undefined);
      await ondeleted();
      forgetRole(role.id);
    } catch (error) {
      problem =
        error instanceof ApiError && error.code === 'last_administrator'
          ? m.lastAdministrator({ role: predefinedRoles.system_admin.name() })
          : describeError(error).message;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.deleteRoleTitle({ name: role.name })} size="sm" dismissible={!busy}>
  <div class="body">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#if role.assignments === 0}
      <p>{m.deleteUnused()}</p>
    {:else}
      <p>{m.deleteRevokes()}</p>
      <ul class="holders">
        {#each listed as assignment (assignment.id)}
          <li>
            <span class="who">{holderName(assignment)}</span> ·
            <span class="scope">
              {assignment.project_id === null
                ? m.globalScope()
                : m.projectScope({ id: assignment.project_id })}
            </span>
          </li>
        {/each}
        {#if more > 0}<li class="more">{m.andMore({ count: more })}</li>{/if}
      </ul>
    {/if}
    <p>{m.cannotUndo()}</p>
  </div>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="danger" loading={busy} onclick={confirm}>{m.deleteRole()}</Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .holders {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .who {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .scope,
  .more {
    color: var(--color-text-muted);
  }
</style>
