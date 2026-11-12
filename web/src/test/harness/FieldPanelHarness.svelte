<script lang="ts">
  // The field panel of a stored issue, as the issue page shows it: edits go through the store.
  import FieldPanel from '#lib/features/issues/fields/FieldPanel.svelte';
  import { issueRow } from '#lib/features/issues/index.ts';

  let { id }: { id: number } = $props();
  const row = $derived(issueRow(id));

  function toggle(): void {
    // The panel stays expanded in these tests.
  }
</script>

{#if row}
  <FieldPanel issue={row} collapsed={false} side="end" ontoggle={toggle} />
{/if}
