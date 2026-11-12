<script lang="ts">
  // A link from an address-confirmation email (AUTH-014).
  import { accountMessages as m, VerifyEmail } from '#lib/features/account/index.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';
</script>

<svelte:head><title>{m.verifyTitle()} · trackflow</title></svelte:head>

<PublicFrame title={m.verifyTitle()}>
  <VerifyEmail />
</PublicFrame>
