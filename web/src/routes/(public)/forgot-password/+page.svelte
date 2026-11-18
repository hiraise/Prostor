<script lang="ts">
  import { accountMessages as m, ForgotPasswordForm } from '#lib/features/account/index.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

<svelte:head><title>{m.forgotTitle()} · TaskMaster</title></svelte:head>

<PublicFrame title={m.forgotTitle()}>
  <ForgotPasswordForm emailEnabled={data.emailEnabled} />
</PublicFrame>
