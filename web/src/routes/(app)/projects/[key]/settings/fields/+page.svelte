<script lang="ts">
  // A project's fields (FLD-051…FLD-060); the layouts above load the project and check access.
  import { ProjectFieldsPage } from '#lib/features/fields/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const project = $derived(data.project?.current);
</script>

{#if project}
  {#key project.id}<ProjectFieldsPage {project} />{/key}
{/if}
