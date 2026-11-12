<script lang="ts">
  import { goto } from '$app/navigation';

  import { accountMessages as m, ForcedPasswordForm } from '#lib/features/account/index.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  async function done(): Promise<void> {
    toasts.show({ tone: 'success', message: m.passwordChanged() });
    // `next` is checked by the load: a path inside the app.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    await goto(data.next, { replaceState: true });
  }
</script>

<svelte:head><title>{m.forcedTitle()} · trackflow</title></svelte:head>

<PublicFrame title={m.forcedTitle()}>
  <ForcedPasswordForm ondone={done} />
</PublicFrame>
