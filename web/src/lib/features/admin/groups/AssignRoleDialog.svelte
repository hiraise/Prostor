<script lang="ts">
  // Assigning a role to a group (ACC-060): the role with its description and what it allows, and
  // where it applies — globally (for people who change system settings) or in projects they
  // manage, several at once. In projects only roles that give something there are offered, and
  // only their project permissions are listed (ACC-044). A project team holds roles only in its own
  // project (ACC-033). Passing on a permission one lacks is refused, and the refusal names it
  // (ACC-050).
  import { untrack } from 'svelte';

  import type { Group, PermissionKey, Role } from '#lib/api/schema.gen.ts';
  import {
    allRoles,
    globalOnlyPermissions,
    groupName,
    permissions,
    predefinedRoles,
    projectPermissions,
    roleName,
    worksInProjects,
  } from '#lib/features/access/index.ts';
  import type { ProjectChoice } from '#lib/features/projects/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as users from '../users/messages.ts';
  import ScopeField from '../users/ScopeField.svelte';
  import type { ScopeKind } from '../users/users.ts';
  import { assignGroupRole, problemOf, type Problem } from './groups.ts';
  import * as m from './messages.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  interface Props {
    open?: boolean;
    group: Group;
  }

  let { open = $bindable(false), group }: Props = $props();

  const id = $props.id();
  let roles = $state<Role[] | null>(null);
  let globalOnly = $state.raw<ReadonlySet<PermissionKey> | null>(null);
  let loadProblem = $state<string | null>(null);
  let value = $state('');
  let scope = $state<ScopeKind>('global');
  let projects = $state<ProjectChoice[]>([]);
  let error = $state<string | null>(null);
  let projectsError = $state<string | null>(null);
  let busy = $state(false);
  let problem = $state<Problem | null>(null);

  const team = $derived(group.team_of);
  const inProjects = $derived(team !== null || scope === 'projects');

  /** Roles to offer: globally those the group does not hold globally yet. */
  const offered = $derived(
    (roles ?? []).filter((role) =>
      inProjects
        ? globalOnly !== null && worksInProjects(role, globalOnly)
        : !group.roles.some((held) => held.role.id === role.id && held.project_id === null),
    ),
  );
  const options = $derived<SelectOption[]>([
    { value: '', label: m.chooseRoleOption() },
    ...offered.map((role) => ({ value: String(role.id), label: roleName(role) })),
  ]);
  const chosen = $derived(offered.find((role) => String(role.id) === value));
  /** What the chosen role is for; nothing for a custom role without a description. */
  const about = $derived.by(() => {
    if (!chosen) return inProjects ? users.projectRolesOnly() : undefined;
    if (chosen.builtin) return predefinedRoles[chosen.builtin].description();
    return chosen.description.trim() === '' ? undefined : chosen.description;
  });
  const allows = $derived(
    chosen
      ? inProjects && globalOnly
        ? projectPermissions(chosen, globalOnly)
        : chosen.permissions
      : [],
  );

  // Every opening starts over; the roles are read again, since someone may have added one.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      value = '';
      scope = 'global';
      projects = [];
      error = null;
      projectsError = null;
      problem = null;
      void load();
    });
  });

  async function load(): Promise<void> {
    loadProblem = null;
    try {
      const [found, keys] = await Promise.all([allRoles(), globalOnlyPermissions()]);
      roles = found;
      globalOnly = keys;
    } catch (failure) {
      loadProblem = describeError(failure).message;
    }
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    error = chosen ? null : m.chooseRole();
    projectsError =
      team === null && scope === 'projects' && projects.length === 0
        ? users.chooseProjects()
        : null;
    if (!chosen || projectsError) return;
    busy = true;
    try {
      await assignGroupRole(
        group.id,
        chosen.id,
        team !== null
          ? { projects: [team] }
          : scope === 'global'
            ? { global: true }
            : { projects: projects.map((project) => project.id) },
      );
      open = false;
      toasts.show({ tone: 'success', message: m.roleAssigned() });
    } catch (failure) {
      problem = problemOf(failure, { lacking: m.lackingToAssign });
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.assignRoleTitle({ group: groupName(group) })} dismissible={!busy}>
  <form id="{id}-form" class="form" novalidate onsubmit={submit}>
    {#if problem}<ProblemNotice {problem} />{/if}
    {#if loadProblem}
      <Notice tone="danger">{loadProblem}</Notice>
      <div><Button onclick={load}>{common.retry()}</Button></div>
    {:else if !roles}
      <Spinner label={common.loading()} />
    {:else}
      {#if offered.length === 0}
        <p class="note">{m.everyRoleHeld()}</p>
      {:else}
        <Select
          label={m.role()}
          {options}
          bind:value
          hint={about}
          {error}
          required
          onchange={() => {
            error = null;
            problem = null;
          }}
        />
        {#if chosen}
          <section class="permissions" aria-labelledby="{id}-permissions">
            <h3 id="{id}-permissions">{m.rolePermissions()}</h3>
            {#if allows.length > 0}
              <ul>
                {#each allows as key (key)}
                  <li>{permissions[key].name()}</li>
                {/each}
              </ul>
            {:else}
              <p class="note">{m.roleAllowsNothing()}</p>
            {/if}
          </section>
        {/if}
      {/if}
      <ScopeField bind:scope bind:projects {team} error={projectsError} />
    {/if}
  </form>

  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button
      type="submit"
      form="{id}-form"
      variant="primary"
      loading={busy}
      disabled={!roles || offered.length === 0}
    >
      {m.assignRole()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  h3 {
    margin-bottom: var(--space-1);
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }

  /* Two columns where they fit; the dialog scrolls a long list. */
  .permissions ul {
    margin: 0;
    padding-left: var(--space-5);
    columns: 2 13rem;
    column-gap: var(--space-8);
    font-size: var(--text-sm);
  }

  .permissions li {
    break-inside: avoid;
    margin-bottom: var(--space-0_5);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
