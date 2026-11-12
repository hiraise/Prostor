<script lang="ts">
  // The roles a group holds (ACC-060), each with its scope: every member holds them too. Assigning
  // and revoking at the global scope needs `system.write`, in a project `project.update` there. A
  // project team holds roles only in its own project (ACC-033).
  import ShieldMinus from '@lucide/svelte/icons/shield-minus';
  import ShieldPlus from '@lucide/svelte/icons/shield-plus';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';

  import type { Group, GroupRole } from '#lib/api/schema.gen.ts';
  import { groupName, predefinedRoles, roleName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { ProjectLabel } from '#lib/features/projects/index.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import AssignRoleDialog from './AssignRoleDialog.svelte';
  import {
    canAssignGlobally,
    canAssignTo,
    problemOf,
    revokeGroupRole,
    type Problem,
  } from './groups.ts';
  import * as m from './messages.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  let { group }: { group: Group } = $props();

  const id = $props.id();
  let assigning = $state(false);
  let revoking = $state<GroupRole | null>(null);
  let confirmOpen = $state(false);
  let busy = $state(false);
  let problem = $state<Problem | null>(null);

  const mayAssign = $derived(canAssignTo(group));

  /** Revoking needs the same rights as assigning at that scope. */
  function mayRevoke(held: GroupRole): boolean {
    return held.project_id === null
      ? canAssignGlobally()
      : can('system.write') || can('project.update', held.project_id);
  }

  /** What a predefined role is for. */
  function about(held: GroupRole): string | null {
    return held.role.builtin ? predefinedRoles[held.role.builtin].description() : null;
  }

  function askToRevoke(held: GroupRole): void {
    revoking = held;
    problem = null;
    confirmOpen = true;
  }

  async function confirmRevoke(): Promise<void> {
    const held = revoking;
    if (!held) return;
    busy = true;
    problem = null;
    try {
      await revokeGroupRole(group.id, held.assignment_id);
      confirmOpen = false;
      toasts.show({ tone: 'success', message: m.roleRevoked() });
    } catch (error) {
      problem = problemOf(error, { lastAdministrator: m.noAdministratorLeft });
    } finally {
      busy = false;
    }
  }
</script>

<section class="roles" aria-labelledby="{id}-title">
  <div class="head">
    <h2 id="{id}-title">{m.rolesTitle()}</h2>
    {#if mayAssign}
      <Button
        class="assign"
        icon={ShieldPlus}
        onclick={() => {
          assigning = true;
        }}
      >
        {m.assignRole()}
      </Button>
    {/if}
  </div>
  <p class="intro">{m.rolesIntro()}</p>

  {#if group.roles.length === 0}
    <EmptyState icon={ShieldCheck} title={m.noRoles()} description={m.noRolesText()} level={3} />
  {:else}
    <ul class="list" aria-label={m.groupRoles({ name: groupName(group) })}>
      {#each group.roles as held (held.assignment_id)}
        {@const name = roleName(held.role)}
        <li class="role">
          <div class="text">
            <p class="name">{name}</p>
            <p class="details">
              {#if held.project_id === null}
                {m.globalScope()}
              {:else}
                <ProjectLabel id={held.project_id} link logo />
              {/if}
              {#if about(held)}<span class="about"> · {about(held)}</span>{/if}
            </p>
          </div>
          {#if mayRevoke(held)}
            <IconButton
              icon={ShieldMinus}
              label={m.revokeRoleLabel({ role: name })}
              onclick={() => {
                askToRevoke(held);
              }}
            />
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

{#if mayAssign}
  <AssignRoleDialog bind:open={assigning} {group} />
{/if}

<Dialog
  bind:open={confirmOpen}
  title={m.revokeRoleTitle({ role: revoking ? roleName(revoking.role) : '' })}
  size="sm"
  dismissible={!busy}
>
  <div class="confirm">
    {#if problem}<ProblemNotice {problem} />{/if}
    <p>{m.revokeRoleText({ group: groupName(group) })}</p>
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
    <Button variant="danger" loading={busy} onclick={confirmRevoke}>{m.revokeRole()}</Button>
  {/snippet}
</Dialog>

<style>
  .roles {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
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

  .head :global(.assign) {
    margin-left: auto;
  }

  .intro {
    max-width: 44rem;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .role {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
  }

  .role + .role {
    border-top: 1px solid var(--color-border);
  }

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-0_5);
    min-width: 0;
  }

  .name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .details {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
