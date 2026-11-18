<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  import { IssuePage } from '#lib/features/issues/index.ts';
  import * as states from '#lib/i18n/messages/states.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const issue = $derived(data.issue?.current);
</script>

<svelte:head>
  {#if !issue}<title>{states.notFoundTitle()} · {brand()}</title>{/if}
</svelte:head>

{#if issue}
  <IssuePage {issue} requested={data.requested} page={data.page ?? undefined} edit={data.edit} />
{:else}
  <NotFoundState home={homeHref()} />
{/if}
