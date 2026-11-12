<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import type { ValueSet } from '#lib/api/schema.gen.ts';
  import { ValueSetsPage, type SetFilters } from '#lib/features/fields/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import { withSearchParams } from '#lib/url/url.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  function open(set: ValueSet): Promise<void> {
    return goto(resolve('/(app)/admin/value-sets/[id]', { id: String(set.id) }));
  }

  // The search stays in the address without loading the page again (UI-006).
  function remember(filters: SetFilters): void {
    const href = withSearchParams(page.url.href, { q: filters.q, kind: filters.kind });
    // The address is this page's own, with new search parameters.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(href, { shallow: true, replace: true, state: page.state });
  }
</script>

<ValueSetsPage
  sets={data.sets}
  filters={data.filters}
  home={homeHref()}
  oncreated={open}
  onfilter={remember}
/>
