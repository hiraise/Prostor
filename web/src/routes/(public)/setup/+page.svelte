<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  import { goto } from '$app/navigation';

  import { accountMessages as m, SetupForm } from '#lib/features/account/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  // The configuration changed (the setup is done): load everything again on the way home.
  async function done(): Promise<void> {
    toasts.show({ tone: 'success', message: m.setupDone() });
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- homeHref() resolves it
    await goto(homeHref(), { replaceState: true, invalidateAll: true });
  }
</script>

<svelte:head><title>{m.setupTitle()} · {brand()}</title></svelte:head>

<PublicFrame title={m.setupTitle()} width="wide">
  <SetupForm ondone={done} />
</PublicFrame>
