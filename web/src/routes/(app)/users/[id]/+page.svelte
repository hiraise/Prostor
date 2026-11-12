<script lang="ts">
  import { UserProfilePage } from '#lib/features/admin/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

<UserProfilePage profile={data.profile} emailEnabled={data.bootstrap?.email_enabled ?? false} />
