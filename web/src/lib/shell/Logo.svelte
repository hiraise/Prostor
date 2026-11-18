<script lang="ts">
  // The TaskMaster mark (three lanes of work flowing right) and the wordmark. Decorative: the link
  // around it carries the accessible name.
  let { compact = false }: { compact?: boolean } = $props();
</script>

<span class={['logo', { compact }]} aria-hidden="true">
  <svg class="mark" viewBox="0 0 32 32">
    <rect width="32" height="32" rx="7" class="tile" />
    <rect x="7" y="9" width="12" height="4" rx="2" class="lane" />
    <rect x="11" y="15" width="14" height="4" rx="2" class="lane" opacity=".75" />
    <rect x="7" y="21" width="9" height="4" rx="2" class="lane" opacity=".5" />
  </svg>
  {#if !compact}<span class="word">Task<b>Master</b></span>{/if}
</span>

<style>
  .logo {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text);
  }

  .mark {
    width: 1.75rem;
    height: 1.75rem;
  }

  .tile {
    fill: var(--color-mark);
  }

  .lane {
    fill: var(--color-brand);
  }

  .word {
    font-size: var(--text-lg);
    font-weight: var(--weight-regular);
    letter-spacing: -0.01em;
  }

  b {
    font-weight: var(--weight-semibold);
  }
</style>
