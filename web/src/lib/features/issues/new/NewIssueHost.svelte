<script lang="ts">
  // Where the new-issue dialog lives, once in the shell. Its code loads the first time it opens,
  // so the shell stays small (architecture.md §7).
  import { newIssueDialog } from './open.svelte.ts';

  let wanted = $state(false);
  $effect(() => {
    if (newIssueDialog.open) wanted = true;
  });
</script>

{#if wanted}
  {#await import('./NewIssueDialog.svelte') then { default: NewIssueDialog }}
    <NewIssueDialog />
  {/await}
{/if}
