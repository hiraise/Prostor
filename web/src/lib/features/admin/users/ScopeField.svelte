<script lang="ts">
  // Where a role applies (ACC-044, ACC-059, ACC-060): globally, which needs `system.write`, or in
  // projects the viewer manages (`project.update` there), several at once. A project team holds
  // roles only in its own project (ACC-033), so for a team the project is fixed.
  import { ProjectLabel, ProjectPicker, type ProjectChoice } from '#lib/features/projects/index.ts';
  import { can, canAnywhere } from '#lib/session/index.ts';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';

  import * as m from './messages.ts';
  import type { ScopeKind } from './users.ts';

  interface Props {
    scope?: ScopeKind;
    projects?: ProjectChoice[];
    /** The project of a team: the only scope it may hold roles in. */
    team?: number | null;
    /** No project chosen while the scope needs one. */
    error?: string | null;
  }

  let {
    scope = $bindable('global'),
    projects = $bindable([]),
    team = null,
    error = null,
  }: Props = $props();

  const id = $props.id();
  const mayGlobal = $derived(team === null && can('system.write'));
  const mayProjects = $derived(team === null && canAnywhere('project.update'));
  const options = $derived([
    { value: 'global', label: m.scopeGlobal(), hint: m.scopeGlobalHint() },
    { value: 'projects', label: m.scopeProjects(), hint: m.scopeProjectsHint() },
  ]);

  // The scope the viewer may use when only one is open to them.
  $effect(() => {
    if (team !== null || (scope === 'global' && !mayGlobal && mayProjects)) scope = 'projects';
    else if (scope === 'projects' && !mayProjects && mayGlobal) scope = 'global';
  });
</script>

{#if team !== null}
  <section class="scope" aria-labelledby="{id}-title">
    <h3 id="{id}-title">{m.scope()}</h3>
    <p>{m.teamScope()} <strong><ProjectLabel id={team} /></strong></p>
    <p class="note">{m.teamScopeHint()}</p>
  </section>
{:else if mayGlobal && mayProjects}
  <RadioGroup legend={m.scope()} {options} bind:value={scope} />
{:else}
  <section class="scope" aria-labelledby="{id}-title">
    <h3 id="{id}-title">{m.scope()}</h3>
    <p>{mayGlobal ? m.scopeGlobalHint() : m.scopeProjectsHint()}</p>
  </section>
{/if}

{#if team === null && scope === 'projects' && mayProjects}
  <ProjectPicker label={m.projectsLegend()} bind:chosen={projects} {error} />
{/if}

<style>
  .scope {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  h3 {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }

  strong {
    font-weight: var(--weight-medium);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
