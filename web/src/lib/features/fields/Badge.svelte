<script lang="ts">
  // A short label next to a name: "Built-in", "Private", "Archived". Text carries the meaning; an
  // optional icon and the tint only group it visually (UI-032).
  import type { LucideIcon } from '@lucide/svelte';

  let { icon: Icon, text }: { icon?: LucideIcon; text: string } = $props();
</script>

<span class="badge">
  {#if Icon}<Icon aria-hidden="true" />{/if}
  {text}
</span>

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

  .badge :global(svg) {
    flex: none;
    width: var(--icon-sm);
    height: var(--icon-sm);
  }

  @media (forced-colors: active) {
    .badge {
      border: 1px solid CanvasText;
    }
  }
</style>
