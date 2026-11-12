<script lang="ts">
  // A project's built-in rules (AUT-023); the layouts above load the project and check access.
  import { RulesTab } from '#lib/features/projects/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const project = $derived(data.project?.current);
</script>

{#if project}
  {#key project.id}<RulesTab {project} />{/key}
{/if}
