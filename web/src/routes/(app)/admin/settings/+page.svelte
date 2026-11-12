<script lang="ts">
  import { SettingsPage } from '#lib/features/admin/settings/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

{#if data.maySee}
  <SettingsPage settings={null} />
{:else}
  <SettingsPage settings={undefined} />
{/if}
