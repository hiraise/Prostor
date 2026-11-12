<script lang="ts">
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';

  import { ProjectFrame } from '#lib/features/projects/index.ts';
  import * as states from '#lib/i18n/messages/states.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';

  import type { LayoutData } from './$types';

  let { data, children }: { data: LayoutData; children: Snippet } = $props();

  const project = $derived(data.project?.current);
</script>

<svelte:head>
  {#if !project}<title>{states.notFoundTitle()} · trackflow</title>{/if}
</svelte:head>

{#if project}
  <ProjectFrame {project} pathname={page.url.pathname}>
    {@render children()}
  </ProjectFrame>
{:else}
  <NotFoundState home={homeHref()} />
{/if}
