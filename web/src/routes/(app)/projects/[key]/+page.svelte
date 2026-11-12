<script lang="ts">
  // A project's overview (PRJ-014); the layout loads the project and shows "not found" without it.
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import { openNewIssue } from '#lib/features/issues/index.ts';
  import { ProjectOverview } from '#lib/features/projects/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const project = $derived(data.project?.current);
</script>

{#if project}
  <ProjectOverview
    {project}
    ondeleted={() => goto(resolve('/(app)/projects'))}
    oncreateissue={() => {
      openNewIssue({ project: project.key });
    }}
  />
{/if}
