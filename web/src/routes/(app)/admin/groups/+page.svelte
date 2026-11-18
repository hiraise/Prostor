<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import type { Group } from '#lib/api/schema.gen.ts';
  import type { GroupKind } from '#lib/features/access/index.ts';
  import { adminMessages as m, GroupsPage } from '#lib/features/admin/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import { withSearchParams } from '#lib/url/url.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  function open(group: Group): Promise<void> {
    return goto(resolve('/(app)/admin/groups/[id]', { id: String(group.id) }));
  }

  // The search and the kind stay in the address without loading the page again (UI-006).
  function remember(text: string, kind: GroupKind): void {
    const href = withSearchParams(page.url.href, { q: text, kind: kind === 'groups' ? '' : kind });
    // The address is this page's own, with a new search parameter.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(href, { shallow: true, replace: true, state: page.state });
  }
</script>

<svelte:head><title>{m.groups()} · TaskMaster</title></svelte:head>

{#if data.groups}
  <GroupsPage
    groups={data.groups}
    query={data.query}
    kind={data.kind}
    oncreated={open}
    onsearch={remember}
  />
{:else}
  <ForbiddenState home={homeHref()} />
{/if}
