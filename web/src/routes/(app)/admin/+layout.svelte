<script lang="ts">
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';

  import { AdminNav } from '#lib/features/admin/index.ts';

  let { children }: { children: Snippet } = $props();
</script>

<AdminNav pathname={page.url.pathname} />
{@render children()}
