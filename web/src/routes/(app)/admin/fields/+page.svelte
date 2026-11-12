<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import type { Field } from '#lib/api/schema.gen.ts';
  import { FieldsPage, type FieldFilters } from '#lib/features/fields/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import { withSearchParams } from '#lib/url/url.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  function open(field: Field): Promise<void> {
    return goto(resolve('/(app)/admin/fields/[id]', { id: String(field.id) }));
  }

  // The search stays in the address without loading the page again (UI-006).
  function remember(filters: FieldFilters): void {
    const href = withSearchParams(page.url.href, {
      q: filters.q,
      unused: filters.unused ? 'true' : null,
    });
    // The address is this page's own, with new search parameters.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(href, { shallow: true, replace: true, state: page.state });
  }
</script>

<FieldsPage
  fields={data.fields}
  filters={data.filters}
  home={homeHref()}
  oncreated={open}
  onfilter={remember}
/>
