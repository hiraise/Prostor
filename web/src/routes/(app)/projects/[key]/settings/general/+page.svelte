<script lang="ts">
  // A project's general settings (PRJ-038); the settings layout checks who may open them.
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import type { Project } from '#lib/api/schema.gen.ts';
  import { GeneralSettings, projectHref } from '#lib/features/projects/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const project = $derived(data.project?.current);

  // The old key keeps working, but the address shows the current one.
  function moved(saved: Project): Promise<void> {
    // projectHref resolves the path.
    return goto(projectHref.settings(saved), { replace: true, reset: false });
  }
</script>

{#if project}
  <GeneralSettings
    {project}
    ondeleted={() => goto(resolve('/(app)/projects'))}
    onkeychanged={moved}
  />
{/if}
