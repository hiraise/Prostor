<script lang="ts">
  // A button with only an icon. `label` is its accessible name and its tooltip, so the two never
  // disagree (UI-032).
  import type { LucideIcon } from '@lucide/svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  import type { Placement } from './position.ts';
  import { tooltip } from './tooltip.ts';

  interface Props extends Omit<HTMLButtonAttributes, 'children'> {
    icon: LucideIcon;
    label: string;
    variant?: 'ghost' | 'secondary';
    size?: 'sm' | 'md' | 'lg';
    /** A toggle button's state (`aria-pressed`). */
    pressed?: boolean;
    tooltipPlacement?: Placement;
  }

  let {
    icon: Icon,
    label,
    variant = 'ghost',
    size = 'md',
    pressed,
    tooltipPlacement,
    type = 'button',
    class: className,
    ...rest
  }: Props = $props();
</script>

<button
  {type}
  class={['icon-button', variant, size, className]}
  aria-label={label}
  aria-pressed={pressed}
  {@attach tooltip(label, tooltipPlacement)}
  {...rest}
>
  <Icon aria-hidden="true" />
</button>

<style>
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-md);
    height: var(--control-md);
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .icon-button :global(svg) {
    width: var(--icon-lg);
    height: var(--icon-lg);
  }

  .icon-button:hover,
  .icon-button[aria-expanded='true'] {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .icon-button[aria-pressed='true'] {
    background: var(--color-selected);
    color: var(--color-text);
  }

  .secondary {
    border-color: var(--color-border-control);
    background: var(--color-surface);
  }

  .sm {
    width: var(--control-sm);
    height: var(--control-sm);
  }

  .sm :global(svg) {
    width: var(--icon-md);
    height: var(--icon-md);
  }

  .lg {
    width: var(--control-lg);
    height: var(--control-lg);
  }

  .icon-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  @media (forced-colors: active) {
    .icon-button {
      border-color: ButtonText;
    }
  }
</style>
