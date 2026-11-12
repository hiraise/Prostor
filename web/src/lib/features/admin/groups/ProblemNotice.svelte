<script lang="ts">
  // A failed change inside a dialog or form: what happened and, for a refusal that names them,
  // the missing permissions by name (ACC-050).
  import Notice from '#lib/ui/Notice.svelte';

  import type { Problem } from './groups.ts';

  let { problem }: { problem: Problem } = $props();
</script>

<Notice tone="danger">
  <p>{problem.message}</p>
  {#if problem.missing.length > 0}
    <ul class="missing">
      {#each problem.missing as name (name)}
        <li>{name}</li>
      {/each}
    </ul>
  {/if}
</Notice>

<style>
  .missing {
    margin: var(--space-1) 0 0;
    padding-left: var(--space-5);
  }
</style>
