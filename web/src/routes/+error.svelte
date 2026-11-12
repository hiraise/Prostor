<script lang="ts">
  // Errors outside the signed-in app (an unknown address, a failed public page): the same states
  // as inside it, in the public frame (UI-036).
  import { refreshAll } from '$app/navigation';
  import { page } from '$app/state';

  import * as states from '#lib/i18n/messages/states.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';
  import { homeHref } from '#lib/shell/sections.ts';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
</script>

<svelte:head>
  <title
    >{page.status === 404 ? states.notFoundTitle() : states.loadFailedTitle()} · trackflow</title
  >
</svelte:head>

<PublicFrame>
  {#if page.status === 404}
    <NotFoundState home={homeHref()} />
  {:else}
    <ErrorState error={page.error} level={1} onretry={() => refreshAll()} />
  {/if}
</PublicFrame>
