<script lang="ts">
  import { AccountPage } from '#lib/features/account/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
</script>

{#if data.account}
  <AccountPage
    account={data.account}
    tokens={data.tokens}
    emailEnabled={data.emailEnabled}
    gravatar={data.gravatar}
  />
{/if}
