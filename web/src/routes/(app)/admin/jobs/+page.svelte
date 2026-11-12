<script lang="ts">
  import { JobsPage } from '#lib/features/admin/jobs/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
</script>

<JobsPage home={homeHref()} />
