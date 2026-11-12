<script lang="ts">
  // Boards keep their header filter while the person stays under `/boards` (BRD-070).
  import type { Snippet } from 'svelte';

  import { beforeNavigate } from '$app/navigation';

  import { headerFilter, underBoards } from '#lib/features/boards/index.ts';

  let { children }: { children: Snippet } = $props();

  beforeNavigate(({ to }) => {
    if (to && !underBoards(to.url.pathname)) headerFilter.clear();
  });
</script>

{@render children()}
