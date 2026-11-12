<script lang="ts">
  // A short label next to a name: "Built-in", "Auto-join", a role. Text carries the meaning; the
  // tint only groups it visually.
  import type { Snippet } from 'svelte';

  let { tone = 'neutral', children }: { tone?: 'neutral' | 'info'; children: Snippet } = $props();
</script>

<span class={['badge', tone]}>{@render children()}</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
    padding: 0.0625rem var(--space-2);
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-normal);
    overflow-wrap: anywhere;
  }

  .info {
    background: var(--color-info-bg);
    color: var(--color-info);
  }

  @media (forced-colors: active) {
    .badge {
      border: 1px solid CanvasText;
    }
  }
</style>
