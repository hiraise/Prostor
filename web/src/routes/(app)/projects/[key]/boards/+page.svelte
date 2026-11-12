<script lang="ts">
  import { BoardList } from '#lib/features/boards/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const project = $derived(data.project?.current);
</script>

{#if project && data.first}
  {#key data}
    <BoardList first={data.first} {project} />
  {/key}
{/if}
