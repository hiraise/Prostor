<script lang="ts">
  // A section whose screens are not built yet: its title and an honest empty state.
  import Construction from '@lucide/svelte/icons/construction';

  import EmptyState from '#lib/ui/EmptyState.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';

  import * as m from './messages.ts';

  let { title }: { title: string } = $props();
</script>

<svelte:head><title>{title} · TaskMaster</title></svelte:head>

<PageHeader {title} />
<EmptyState
  icon={Construction}
  title={m.sectionInProgress()}
  description={m.sectionInProgressText()}
/>
