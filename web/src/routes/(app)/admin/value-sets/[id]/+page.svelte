<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import { ValueSetPage } from '#lib/features/fields/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

<ValueSetPage
  set={data.set}
  home={homeHref()}
  ondeleted={() => goto(resolve('/(app)/admin/value-sets'))}
/>
