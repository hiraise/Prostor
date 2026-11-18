<script lang="ts">
  import { page } from '$app/state';

  import { BoardPage } from '#lib/features/boards/index.ts';
  import * as states from '#lib/i18n/messages/states.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const board = $derived(data.board?.current);
</script>

<svelte:head>
  {#if !board}<title>{states.notFoundTitle()} · TaskMaster</title>{/if}
</svelte:head>

{#if board && data.sprint !== null}
  {#key `${String(board.id)}/${String(data.sprint)}`}
    <BoardPage
      {board}
      sprint={data.sprint}
      q={page.url.searchParams.get('q')}
      card={page.url.searchParams.get('issue')}
    />
  {/key}
{:else}
  <NotFoundState home={homeHref()} />
{/if}
