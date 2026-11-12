<script lang="ts">
  import { goto } from '$app/navigation';

  import { IssueList, listHref, type ListParams } from '#lib/features/issues/index.ts';

  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  // Every search is a new address (QRY-069); focus stays in the bar.
  function open(params: ListParams): void {
    // listHref resolves the address.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(listHref(params), { reset: false });
  }
</script>

<IssueList
  q={data.q}
  context={data.context}
  list={data.list}
  error={data.error}
  onnavigate={open}
/>
