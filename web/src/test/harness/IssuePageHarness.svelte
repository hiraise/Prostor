<script lang="ts">
  // The issue page of a stored issue, as the route shows it: edits and live changes go through the
  // store, and an issue gone from it is no longer shown.
  import { IssuePage, issueRow, type IssuePageData } from '#lib/features/issues/index.ts';
  import type { LiveSource } from '#lib/features/issues/page/live.svelte.ts';

  let { id, page, connection }: { id: number; page?: IssuePageData; connection?: LiveSource } =
    $props();
  const row = $derived(issueRow(id));
</script>

{#if row}<IssuePage issue={row} {page} {connection} />{:else}<p>Gone</p>{/if}
