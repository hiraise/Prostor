<script lang="ts">
  // The projects of a person (ACC-002, ACC-033): projects whose team includes them, directly or
  // through a group, among those the viewer may see. Each opens the project; its team changes on
  // the project's people page.
  import type { ProjectRef } from '#lib/api/schema.gen.ts';
  import { ProjectLogo, projectHref } from '#lib/features/projects/index.ts';

  import * as m from './messages.ts';

  let { projects }: { projects: readonly ProjectRef[] } = $props();

  const headingId = $props.id();
</script>

<section class="section" aria-labelledby={headingId}>
  <h2 id={headingId}>{m.projectsTitle()}</h2>
  <p class="hint">{m.projectsIntro()}</p>
  {#if projects.length === 0}
    <p class="hint">{m.noProjects()}</p>
  {:else}
    <ul class="projects">
      {#each projects as project (project.id)}
        <li class="project">
          <ProjectLogo
            projectKey={project.key}
            color={project.logo_color}
            src={project.logo_url ?? null}
            size="sm"
          />
          <!-- projectHref resolves the path. -->
          <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
          <a class="name" href={projectHref.overview(project)}>{project.name}</a>
          <span class="key">{project.key}</span>
          {#if project.archived}<span class="badge">{m.archivedBadge()}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .projects {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .project {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-height: calc(var(--control-md) + var(--space-2));
    padding: var(--space-1) var(--space-4);
  }

  .project + .project {
    border-top: 1px solid var(--color-border);
  }

  .name {
    display: inline-flex;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    color: var(--color-link);
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .key {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .badge {
    padding: 0 var(--space-1_5);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }
</style>
