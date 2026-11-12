<script lang="ts">
  // Adding a team role (PRJ-051, ACC-033): every team member gets it in this project at once. Only
  // roles that give something in a project are offered; passing on a permission one lacks is
  // refused, and the refusal names it (ACC-050).
  import { untrack } from 'svelte';

  import type { PermissionKey, Project, ProjectPeople, Role } from '#lib/api/schema.gen.ts';
  import {
    permissions,
    predefinedRoles,
    projectPermissions,
    roleName,
    worksInProjects,
  } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { assignProjectRole } from '../projects.ts';
  import * as m from './messages.ts';
  import { problemOf, teamRoles, type Problem } from './people.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  interface Props {
    open?: boolean;
    project: Project;
    people: ProjectPeople;
    roles: Role[] | null;
    globalOnly: ReadonlySet<PermissionKey> | null;
    ondone: () => unknown;
  }

  let { open = $bindable(false), project, people, roles, globalOnly, ondone }: Props = $props();

  const id = $props.id();
  let value = $state('');
  let error = $state<string | null>(null);
  let busy = $state(false);
  let problem = $state<Problem | null>(null);

  const held = $derived(teamRoles(people).map((assignment) => assignment.role.id));
  const offered = $derived(
    roles && globalOnly
      ? roles.filter((role) => !held.includes(role.id) && worksInProjects(role, globalOnly))
      : [],
  );
  const options = $derived<SelectOption[]>([
    { value: '', label: m.chooseRoleOption() },
    ...offered.map((role) => ({ value: String(role.id), label: roleName(role) })),
  ]);
  const chosen = $derived(offered.find((role) => String(role.id) === value));
  const about = $derived.by(() => {
    if (!chosen) return undefined;
    if (chosen.builtin) return predefinedRoles[chosen.builtin].description();
    return chosen.description.trim() === '' ? undefined : chosen.description;
  });
  const allows = $derived(chosen && globalOnly ? projectPermissions(chosen, globalOnly) : []);

  $effect(() => {
    if (!open) return;
    untrack(() => {
      value = '';
      error = null;
      problem = null;
    });
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    if (!chosen) {
      error = m.chooseRole();
      return;
    }
    error = null;
    busy = true;
    try {
      await assignProjectRole(project.id, chosen.id, { group_id: people.team.id });
      open = false;
      toasts.show({ tone: 'success', message: m.teamRoleAdded() });
      await ondone();
    } catch (failure) {
      problem = problemOf(failure, m.lackingToAssign);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.addTeamRoleTitle()} dismissible={!busy}>
  <form id="{id}-form" class="form" novalidate onsubmit={submit}>
    {#if problem}<ProblemNotice {problem} />{/if}
    {#if !roles || !globalOnly}
      <Spinner label={common.loading()} />
    {:else if offered.length === 0}
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
        <section class="permissions" aria-labelledby="{id}-allows">
          <h3 id="{id}-allows">{m.inProject()}</h3>
          <ul>
            {#each allows as key (key)}<li>{permissions[key].name()}</li>{/each}
          </ul>
        </section>
      {/if}
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
      disabled={offered.length === 0}
    >
      {m.addTeamRole()}
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
