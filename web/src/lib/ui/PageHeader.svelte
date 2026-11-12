<script lang="ts">
  // The top of a page: its title (the page's only h1) and page-level actions. Actions wrap under
  // the title on phones.
  import type { Snippet } from 'svelte';

  let { title, actions }: { title: string; actions?: Snippet } = $props();
</script>

<header class="page-header">
  <h1>{title}</h1>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</header>

<style>
  .page-header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-5) var(--space-6) var(--space-3);
  }

  h1 {
    font-size: var(--text-xl);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  @media (width < 48rem) {
    .page-header {
      padding: var(--space-4) var(--space-4) var(--space-2);
    }
  }
</style>
