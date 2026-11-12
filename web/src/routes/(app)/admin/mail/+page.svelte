<script lang="ts">
  import { MailPage } from '#lib/features/admin/mail/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

{#if data.maySee}
  <MailPage settings={null} />
{:else}
  <MailPage settings={undefined} />
{/if}
