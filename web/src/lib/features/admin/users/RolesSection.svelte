<script lang="ts">
  // The roles of a person per scope with where each comes from (ACC-018, ACC-059, ACC-061): given
  // directly, which can be revoked here, through a group, which ends by leaving the group, or
  // through a project's team (ACC-033), which ends by leaving the team. Each role lists the
  // permissions it gives there. Roles are assigned globally by people who change system settings
  // and in projects by the people who manage them.
  import Plus from '@lucide/svelte/icons/plus';
  import { resolve } from '$app/paths';

  import type { EffectiveRole } from '#lib/api/schema.gen.ts';
  import { groupName, permissions, personName, roleName } from '#lib/features/access/index.ts';
  import { ProjectLabel } from '#lib/features/projects/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can, canAnywhere, canSeeGroups } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import AssignRoleDialog from './AssignRoleDialog.svelte';
  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import { revokeAssignment, rolesOf, type DirectoryAccount } from './users.ts';

  let { account }: { account: DirectoryAccount } = $props();

  const headingId = $props.id();
  let roles = $state<EffectiveRole[] | null>(null);
  let failure = $state<unknown>(null);
  let request = 0;

  let assigning = $state(false);
  let revoking = $state<{ role: EffectiveRole; assignment: number } | null>(null);
  let revokeOpen = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);

  const mayAssign = $derived(
    !account.guest && (can('system.write') || canAnywhere('project.update')),
  );

  /** Revoking needs the same rights as assigning at that scope. */
  function mayRevoke(held: EffectiveRole): boolean {
    if (account.guest) return false;
    return held.project_id === null
      ? can('system.write')
      : can('system.write') || can('project.update', held.project_id);
  }
  /** Roles held directly everywhere: assigning them again changes nothing. */
  const heldDirectly = $derived(
    (roles ?? [])
      .filter(
        (role) =>
          role.project_id === null && role.sources.some((source) => source.kind === 'direct'),
      )
      .map((role) => role.role.id),
  );

  async function load(id: number): Promise<void> {
    const mine = ++request;
    failure = null;
    try {
      const found = await rolesOf(id);
      if (mine === request) roles = found;
    } catch (error) {
      if (mine === request) failure = error;
    }
  }

  $effect(() => {
    void load(account.id);
  });

  function askToRevoke(role: EffectiveRole, assignment: number): void {
    revoking = { role, assignment };
    problem = null;
    revokeOpen = true;
  }

  async function revoke(): Promise<void> {
    const target = revoking;
    if (!target) return;
    busy = true;
    problem = null;
    try {
      await revokeAssignment(target.assignment);
      revokeOpen = false;
      toasts.show({ tone: 'success', message: m.roleRevoked() });
      await load(account.id);
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

<section class="section" aria-labelledby={headingId}>
  <div class="head">
    <h2 id={headingId}>{m.rolesTitle()}</h2>
    {#if mayAssign}
      <Button icon={Plus} onclick={() => (assigning = true)}>{m.assignRole()}</Button>
    {/if}
  </div>
  <p class="hint">{m.rolesIntro()}</p>

  {#if failure}
    <ErrorState error={failure} level={3} onretry={() => load(account.id)} />
  {:else if roles === null}
    <p class="hint"><Spinner label={common.loading()} /></p>
  {:else if roles.length === 0}
    <p class="hint">{m.noRoles()}</p>
  {:else}
    <ul class="roles">
      {#each roles as held (`${String(held.role.id)}:${String(held.project_id)}`)}
        <li class="role">
          <div class="title">
            <h3 class="name">{roleName(held.role)}</h3>
            <span class="scope">
              {#if held.project_id === null}
                {m.globalScope()}
              {:else}
                <ProjectLabel id={held.project_id} link logo />
              {/if}
            </span>
          </div>
          <ul class="sources">
            {#each held.sources as source, index (index)}
              <li class="source">
                {#if source.kind === 'direct'}
                  <span class="source-text">{m.sourceDirect()}</span>
                  {#if mayRevoke(held)}
                    <Button
                      variant="ghost"
                      aria-label={m.revokeLabel({ role: roleName(held.role) })}
                      onclick={() => {
                        askToRevoke(held, source.assignment_id);
                      }}
                    >
                      {m.revoke()}
                    </Button>
                  {/if}
                {:else if source.group.team_of !== null}
                  <span class="source-text">
                    {m.sourceTeam()}
                    <ProjectLabel id={source.group.team_of} link />
                  </span>
                {:else}
                  <span class="source-text">
                    {m.sourceGroup()}
                    {#if canSeeGroups()}
                      <a href={resolve('/(app)/admin/groups/[id]', { id: String(source.group.id) })}
                        >{groupName(source.group)}</a
                      >
                    {:else}
                      <strong>{groupName(source.group)}</strong>
                    {/if}
                  </span>
                {/if}
              </li>
            {/each}
          </ul>
          <details class="permissions">
            <summary>{m.permissionCount({ count: held.permissions.length })}</summary>
            <ul>
              {#each held.permissions as key (key)}<li>{permissions[key].name()}</li>{/each}
            </ul>
          </details>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<AssignRoleDialog
  bind:open={assigning}
  {account}
  held={heldDirectly}
  onassigned={() => load(account.id)}
/>

<Dialog
  bind:open={revokeOpen}
  title={revoking
    ? m.revokeTitle({ role: roleName(revoking.role.role), name: personName(account) })
    : ''}
  size="sm"
  dismissible={!busy}
>
  <div class="body">
    <p>{m.revokeText()}</p>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  </div>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        revokeOpen = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="danger" loading={busy} onclick={revoke}>{m.revoke()}</Button>
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

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .roles {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .role {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4);
  }

  .role + .role {
    border-top: 1px solid var(--color-border);
  }

  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1) var(--space-3);
  }

  .name {
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }

  .scope {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .sources {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .source {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-height: var(--control-md);
    font-size: var(--text-sm);
  }

  .source-text {
    color: var(--color-text-muted);
  }

  .source a {
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    min-width: var(--control-md);
    color: var(--color-link);
  }

  .source strong {
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .permissions summary {
    display: flex;
    align-items: center;
    min-height: var(--control-md);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .permissions summary::before {
    content: '▸';
    width: 1.25em;
  }

  .permissions[open] summary::before {
    content: '▾';
  }

  .permissions ul {
    margin: 0;
    padding-inline-start: var(--space-6);
    font-size: var(--text-sm);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
