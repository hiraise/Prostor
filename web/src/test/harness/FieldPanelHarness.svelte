<script lang="ts">
  // The field panel of a stored issue, as the issue page shows it: edits go through the store.
  import FieldPanel from '#lib/features/issues/fields/FieldPanel.svelte';
  import { issueRow } from '#lib/features/issues/index.ts';

  let { id }: { id: number } = $props();
  const row = $derived(issueRow(id));
</script>

{#if row}
  <FieldPanel issue={row} />
{/if}
