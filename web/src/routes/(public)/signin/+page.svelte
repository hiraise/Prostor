<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';

  import { accountMessages as m, SignInForm } from '#lib/features/account/index.ts';
  import { onSignInElsewhere } from '#lib/session/index.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  // `next` is checked by the load: a path inside the app.
  // eslint-disable-next-line svelte/no-navigation-without-resolve
  const proceed = (): Promise<void> => goto(data.next, { replaceState: true });

  // Signed in from another tab: this one follows.
  onMount(() => onSignInElsewhere(() => void proceed()));
</script>

<svelte:head><title>{m.signInTitle()}</title></svelte:head>

<PublicFrame title={m.signInTitle()}>
  <SignInForm reason={data.reason} emailEnabled={data.emailEnabled} onsignedin={proceed} />
</PublicFrame>
