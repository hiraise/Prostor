<script lang="ts">
  // Errors of the app's pages and their loads (UI-036): an unknown address is "not found", anything
  // else is a failed load with Try again.
  import { refreshAll } from '$app/navigation';
  import { page } from '$app/state';

  import * as states from '#lib/i18n/messages/states.ts';
  import { brand } from '#lib/i18n/messages/brand.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
</script>

<svelte:head>
  <title
    >{page.status === 404 ? states.notFoundTitle() : states.loadFailedTitle()} · {brand()}</title
  >
</svelte:head>

{#if page.status === 404}
  <NotFoundState home={homeHref()} />
{:else}
  <ErrorState error={page.error} level={1} onretry={() => refreshAll()} />
{/if}
