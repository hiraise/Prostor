<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import { groupName } from '#lib/features/access/index.ts';
  import { GroupPage } from '#lib/features/admin/index.ts';
  import * as states from '#lib/i18n/messages/states.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const group = $derived(data.group?.current);
  const title = $derived(
    !data.allowed ? states.forbiddenTitle() : group ? groupName(group) : states.notFoundTitle(),
  );
</script>

<svelte:head><title>{title} · {brand()}</title></svelte:head>

{#if !data.allowed}
  <ForbiddenState home={homeHref()} />
{:else if data.group}
  <GroupPage
    group={data.group}
    home={homeHref()}
    ondeleted={() => goto(resolve('/(app)/admin/groups'))}
  />
{:else}
  <NotFoundState home={homeHref()} />
{/if}
