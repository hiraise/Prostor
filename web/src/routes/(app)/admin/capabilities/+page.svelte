<script lang="ts">
  import { CapabilitiesPage } from '#lib/features/admin/capabilities/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

{#if data.maySee}
  <CapabilitiesPage capabilities={null} />
{:else}
  <CapabilitiesPage capabilities={undefined} />
{/if}
