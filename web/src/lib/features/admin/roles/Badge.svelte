<script lang="ts">
  // A small marker next to a name: an icon and a word, never color alone (docs/design.md §1).
  import type { LucideIcon } from '@lucide/svelte';

  let { icon: Icon, text, id }: { icon: LucideIcon; text: string; id?: string } = $props();
</script>

<span class="badge" {id}><Icon aria-hidden="true" />{text}</span>

<style>
  .badge {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-1_5);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: 1.5;
    white-space: nowrap;
  }

  .badge :global(svg) {
    width: var(--icon-sm);
    height: var(--icon-sm);
  }

  @media (forced-colors: active) {
    .badge {
      border-color: CanvasText;
    }
  }
</style>
