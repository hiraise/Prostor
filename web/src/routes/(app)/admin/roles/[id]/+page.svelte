<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import { RolePage } from '#lib/features/admin/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

<RolePage
  role={data.role}
  catalog={data.catalog}
  holders={data.holders}
  home={homeHref()}
  ondeleted={() => goto(resolve('/(app)/admin/roles'))}
/>
