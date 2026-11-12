<script lang="ts">
  // One's own groups, projects and roles (ACC-018, ACC-061): the groups one is in, the projects
  // whose team one is in, and every role per scope — global or a project — with where it comes
  // from: given directly, through a group, or through a project's team (ACC-033); and the
  // permissions it gives there.
  import { onMount } from 'svelte';

  import { api } from '#lib/api/client.ts';
  import type { Account, EffectiveRole } from '#lib/api/schema.gen.ts';
  import {
    builtinGroups,
    groupName,
    permissions,
    predefinedRoles,
    roleName,
  } from '#lib/features/access/index.ts';
  import { ProjectLabel, ProjectLogo, projectHref } from '#lib/features/projects/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';

  import * as m from './messages.ts';

  let { account }: { account: Account } = $props();

  let roles = $state<EffectiveRole[] | null>(null);
  let failure = $state<unknown>(null);
  let shown = $state<EffectiveRole | null>(null);
  let dialogOpen = $state(false);

  const groups = $derived([
    builtinGroups.everyone.name(),
    ...(account.guest ? [] : [builtinGroups.all_members.name()]),
    ...account.groups.filter((group) => group.team_of === null).map(groupName),
  ]);

  async function load(): Promise<void> {
    failure = null;
    try {
      roles = (await api.get('/users/{id}/roles', { path: { id: account.id } })).items;
    } catch (error) {
      failure = error;
    }
  }

  onMount(() => void load());
</script>

<div class="access">
  <p class="hint">{m.accessIntro()}</p>
  <section aria-labelledby="my-groups">
    <h3 id="my-groups">{m.myGroups()}</h3>
    <ul class="chips">
      {#each groups as group (group)}<li>{group}</li>{/each}
    </ul>
  </section>
  {#if account.projects.length > 0}
    <section aria-labelledby="my-projects">
      <h3 id="my-projects">{m.myProjects()}</h3>
      <ul class="chips">
        {#each account.projects as project (project.id)}
          <li class="project">
            <!-- projectHref resolves the path. -->
            <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
            <a href={projectHref.overview(project)}>
              <ProjectLogo
                projectKey={project.key}
                color={project.logo_color}
                src={project.logo_url ?? null}
                size="sm"
              />
              {project.name}
            </a>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
  <section aria-labelledby="my-roles">
    <h3 id="my-roles">{m.myRoles()}</h3>
    {#if failure}
      <ErrorState error={failure} level={3} onretry={load} />
    {:else if roles === null}
      <Spinner />
    {:else if roles.length === 0}
      <p class="hint">{m.noRoles()}</p>
    {:else}
      <ul class="roles">
        {#each roles as role (`${String(role.role.id)}:${String(role.project_id)}`)}
          <li class="role">
            <div class="text">
              <p class="name">{roleName(role.role)}</p>
              <p class="meta">
                {#if role.project_id === null}{m.globalScope()}{:else}<ProjectLabel
                    id={role.project_id}
                    link
                  />{/if} ·
                {#each role.sources as item, index (index)}{#if index > 0},
                  {/if}{#if item.kind === 'direct'}{m.sourceDirect()}{:else if item.group.team_of !== null}{m.sourceTeam()}
                    <ProjectLabel id={item.group.team_of} link />{:else}{m.sourceGroup({
                      group: groupName(item.group),
                    })}{/if}{/each}
              </p>
              {#if role.role.builtin}
                <p class="meta">{predefinedRoles[role.role.builtin].description()}</p>
              {/if}
            </div>
            <Button
              size="sm"
              variant="ghost"
              onclick={() => {
                shown = role;
                dialogOpen = true;
              }}
            >
              {m.showPermissions({ role: roleName(role.role) })}
            </Button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<Dialog
  bind:open={dialogOpen}
  title={shown ? m.showPermissions({ role: roleName(shown.role) }) : ''}
  size="sm"
>
  {#if shown}
    <ul class="permissions">
      {#each shown.permissions as key (key)}
        <li>
          <p class="name">{permissions[key].name()}</p>
          <p class="meta">{permissions[key].description()}</p>
        </li>
      {/each}
    </ul>
  {/if}
</Dialog>

<style>
  .access {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  h3 {
    font-size: var(--text-md);
  }

  .hint,
  .meta {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chips li:not(.project) {
    padding: var(--space-0_5) var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    font-size: var(--text-sm);
  }

  .project a {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1_5);
    min-height: var(--control-md);
    padding: 0 var(--space-3) 0 var(--space-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-link);
    font-size: var(--text-sm);
  }

  .roles,
  .permissions {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .roles {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  .role {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-3) var(--space-4);
  }

  .role + .role {
    border-top: 1px solid var(--color-border);
  }

  .text {
    min-width: 0;
  }

  .name {
    font-weight: var(--weight-medium);
  }

  .permissions {
    gap: var(--space-3);
  }
</style>
