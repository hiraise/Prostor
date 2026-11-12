<script lang="ts">
  // Where the new-board dialog lives, once in the shell. Its code loads the first time it opens,
  // so the shell stays small (architecture.md §7).
  import { newBoardDialog } from './open.svelte.ts';

  let wanted = $state(false);
  $effect(() => {
    if (newBoardDialog.open) wanted = true;
  });
</script>

{#if wanted}
  {#await import('./CreateBoardDialog.svelte') then { default: CreateBoardDialog }}
    <CreateBoardDialog />
  {/await}
{/if}
