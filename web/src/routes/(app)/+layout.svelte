<script lang="ts">
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { page } from '$app/state';

  import {
    changePasswordHref,
    onPasswordChangeRequired,
    onSessionEnd,
    signInHref,
  } from '#lib/session/index.ts';
  import AppShell from '#lib/shell/AppShell.svelte';
  import { followInstance } from '#lib/shell/instance-live.ts';

  let { children }: { children: Snippet } = $props();

  // A session that ends (signed out in any tab, expired, revoked) leaves for the sign-in page with
  // a full page load, so nothing of the previous person stays in memory (AUTH-005).
  onMount(() => {
    const stopEnd = onSessionEnd((reason) => {
      const here = page.url.pathname + page.url.search;
      location.assign(signInHref(reason === 'expired' ? here : undefined, reason));
    });
    // An administrator asked for a new password while this page was open (AUTH-010).
    const stopChange = onPasswordChangeRequired(() => {
      location.assign(changePasswordHref(page.url.pathname + page.url.search));
    });
    // Settings and permissions changed by others reach this tab (ADM-002, ACC-049).
    const stopInstance = followInstance();
    return () => {
      stopEnd();
      stopChange();
      stopInstance();
    };
  });
</script>

<AppShell pathname={page.url.pathname}>
  {@render children()}
</AppShell>
