<script lang="ts">
  // A project named by its ID wherever only the ID is known (scopes of roles, ACC-059…ACC-061):
  // its name from the store, loaded when missing, optionally with its logo and a link to its
  // overview. A project the viewer may not see reads "Hidden project".
  import type { Project } from '#lib/api/schema.gen.ts';
  import { store, type EntityHandle } from '#lib/store/index.ts';

  import ProjectLogo from '../ProjectLogo.svelte';
  import { projectHref } from '../projects.ts';
  import * as m from './messages.ts';

  interface Props {
    id: number;
    /** Links the name to the project's overview. */
    link?: boolean;
    logo?: boolean;
  }

  let { id, link = false, logo = false }: Props = $props();

  let handle = $state.raw<EntityHandle<Project> | null>(null);
  let missing = $state(false);

  $effect(() => {
    const wanted = id;
    let current = true;
    missing = false;
    store.ensureEntity('project', wanted, { form: 'full', staleTimeMs: 60_000 }).then(
      (found) => {
        if (current) handle = found;
      },
      () => {
        if (current) missing = true;
      },
    );
    return () => {
      current = false;
    };
  });

  const project = $derived(handle?.id === id ? handle.current : undefined);
</script>

{#if project}
  <span class="project">
    {#if logo}<ProjectLogo
        projectKey={project.key}
        color={project.logo_color}
        src={project.logo_url ?? null}
        size="sm"
      />{/if}
    {#if link}
      <!-- projectHref resolves the path. -->
      <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
      <a href={projectHref.overview(project)}>{project.name}</a>
    {:else}
      {project.name}
    {/if}
  </span>
{:else if missing}
  <span class="project">{m.hiddenProject()}</span>
{:else}
  <span class="project pending" aria-busy="true">…</span>
{/if}

<style>
  .project {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1_5);
    min-width: 0;
    overflow-wrap: anywhere;
  }

  /* As tall as a control, so a finger hits it on touch screens too. */
  a {
    display: inline-flex;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    color: var(--color-link);
  }

  .pending {
    color: var(--color-text-muted);
  }
</style>
