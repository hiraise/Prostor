<script lang="ts">
  // A value as every screen shows it (FLD-044): its name on the tint of its color scheme, or plain
  // with an outline when it has no color. The name is always there, so color is never alone.
  let { name, color = null }: { name: string; color?: number | null } = $props();

  const scheme = $derived(color !== null && color >= 0 && color < 12 ? color : null);
</script>

<span
  class={['chip', { plain: scheme === null }]}
  style:--chip-bg={scheme === null ? undefined : `var(--value-${String(scheme)}-bg)`}
  style:--chip-fg={scheme === null ? undefined : `var(--value-${String(scheme)}-fg)`}>{name}</span
>

<style>
  .chip {
    display: inline-block;
    max-width: 100%;
    padding: 0.0625rem var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: var(--chip-bg);
    color: var(--chip-fg);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: var(--leading-normal);
    overflow-wrap: anywhere;
  }

  .plain {
    border-color: var(--color-border-control);
    background: transparent;
    color: var(--color-text);
  }

  @media (forced-colors: active) {
    .chip {
      border-color: CanvasText;
    }
  }
</style>
