<script lang="ts">
  import { BoardList } from '#lib/features/boards/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

{#key data}
  <BoardList first={data.first} />
{/key}
