<script lang="ts">
  import { LinkTypesPage } from '#lib/features/links/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

<LinkTypesPage types={data.types} home={homeHref()} />
