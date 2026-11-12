<script lang="ts">
  // A project's settings (PRJ-037): for people who configure the project. The people page is also
  // open, read-only, to people who may read the project in full (PRJ-009, PRJ-046).
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';

  import {
    canConfigure,
    canSeePeople,
    projectHref,
    SettingsNav,
  } from '#lib/features/projects/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';

  import type { LayoutData } from './$types';

  let { data, children }: { data: LayoutData; children: Snippet } = $props();

  const project = $derived(data.project?.current);
  const allowed = $derived(
    project !== undefined &&
      (canConfigure(project) ||
        (page.url.pathname === projectHref.people(project) && canSeePeople(project))),
  );
</script>

{#if project && allowed}
  <div class="settings">
    <SettingsNav {project} pathname={page.url.pathname} />
    <div class="content">{@render children()}</div>
  </div>
{:else if project}
  <ForbiddenState home={homeHref()} />
{/if}

<style>
  .settings {
    display: flex;
    align-items: flex-start;
  }

  .content {
    flex: 1;
    min-width: 0;
  }

  @media (width < 48rem) {
    .settings {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
