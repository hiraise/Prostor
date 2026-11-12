<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import type { Role } from '#lib/api/schema.gen.ts';
  import { RolesPage, type RoleFilters } from '#lib/features/admin/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import { withSearchParams } from '#lib/url/url.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  function open(role: Role): Promise<void> {
    return goto(resolve('/(app)/admin/roles/[id]', { id: String(role.id) }));
  }

  // The search stays in the address without loading the page again (UI-006).
  function remember(filters: RoleFilters): void {
    const href = withSearchParams(page.url.href, { q: filters.q, permission: filters.permission });
    // The address is this page's own, with new search parameters.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(href, { shallow: true, replace: true, state: page.state });
  }
</script>

<RolesPage
  roles={data.roles}
  catalog={data.catalog}
  filters={data.filters}
  home={homeHref()}
  oncreated={open}
  onfilter={remember}
/>
