<script lang="ts">
  // The people of a project (PRJ-046…PRJ-056); the settings layout lets only its managers in.
  import { PeoplePage } from '#lib/features/projects/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const project = $derived(data.project?.current);
</script>

{#if project}
  <PeoplePage {project} />
{/if}
