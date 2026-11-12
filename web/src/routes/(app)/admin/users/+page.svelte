<script lang="ts">
  import { UsersPage } from '#lib/features/admin/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

<UsersPage directory={data.directory} emailEnabled={data.bootstrap?.email_enabled ?? false} />
