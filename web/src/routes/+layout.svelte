<script lang="ts">
  import '#lib/theme/global.css';

  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { invalidate } from '$app/navigation';

  import { BOOTSTRAP_DEPENDENCY } from '#lib/api/system.ts';
  import { initLocale } from '#lib/i18n/locale.svelte.ts';
  import { initSession } from '#lib/session/index.ts';
  import * as m from '#lib/shell/messages.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';
  import { setInstanceFormat } from '#lib/time/context.svelte.ts';
  import { initTheme } from '#lib/theme/theme.svelte.ts';
  import Announcer from '#lib/ui/Announcer.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Toaster from '#lib/ui/Toaster.svelte';

  import type { LayoutData } from './$types';

  let { data, children }: { data: LayoutData; children: Snippet } = $props();

  // Before the first render, so the first frame already has the right theme and language.
  initTheme();
  initLocale();

  const RETRY_MS = 5000;

  onMount(() => {
    // The static message app.html shows when the app cannot start is no longer needed.
    document.getElementById('boot')?.remove();
    // Sign-ins and sign-outs of other tabs, and sessions the server ended.
    return initSession();
  });

  // While the server is unavailable (starting, updating), keep asking for the configuration.
  $effect(() => {
    if (!data.bootstrapError) return;
    const timer = setTimeout(() => void invalidate(BOOTSTRAP_DEPENDENCY), RETRY_MS);
    return () => {
      clearTimeout(timer);
    };
  });

  // Every date follows the instance's format (ADM-016); a settings save reloads the bootstrap,
  // and this effect spreads the new format over the whole interface at once.
  $effect(() => {
    setInstanceFormat(data.bootstrap?.default_date_format ?? 'locale');
  });
</script>

{#if data.bootstrapError}
  <PublicFrame>
    <ErrorState
      error={data.bootstrapError}
      title={m.serverUnavailable()}
      message={m.serverUnavailableText()}
      level={1}
      onretry={() => invalidate(BOOTSTRAP_DEPENDENCY)}
    />
  </PublicFrame>
{:else}
  {@render children()}
{/if}
<Toaster />
<Announcer />
