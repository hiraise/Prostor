<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';

  import { ProjectsPage } from '#lib/features/projects/index.ts';
  import { withSearchParams } from '#lib/url/url.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  // The search stays in the address without loading the page again (UI-006).
  function remember(text: string): void {
    const href = withSearchParams(page.url.href, { q: text });
    // The address is this page's own, with a new search parameter.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(href, { shallow: true, replace: true, state: page.state });
  }

  function open(href: string): Promise<void> {
    // Hrefs come from projectHref, which resolves them.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    return goto(href);
  }
</script>

<!-- A navigation that loads the page again (a link to the list) starts it over from the address. -->
{#key data}
  <ProjectsPage
    active={data.active}
    deleting={data.deleting}
    query={data.query}
    onsearch={remember}
    onnavigate={open}
  />
{/key}
