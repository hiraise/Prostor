<script lang="ts">
  // A text button, or a link that looks like one when `href` is set (docs/design.md §7).
  // While `loading` it keeps its size and focus but ignores clicks.
  import type { LucideIcon } from '@lucide/svelte';
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  import Spinner from './Spinner.svelte';

  interface Props extends HTMLButtonAttributes {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'sm' | 'md' | 'lg';
    icon?: LucideIcon;
    loading?: boolean;
    /** Renders a link with the look of a button. */
    href?: string;
    /** With `href`: the link saves the file under this name instead of opening it. */
    download?: string;
    /** With `href`: open in another browsing context, e.g. `_blank`. */
    target?: string;
    /** With `href` and `target="_blank"`: `noopener` keeps the page out of `window.opener`. */
    rel?: string;
    children?: Snippet;
  }

  let {
    variant = 'secondary',
    size = 'md',
    icon: Icon,
    loading = false,
    href,
    download,
    target,
    rel,
    type = 'button',
    disabled = false,
    children,
    onclick,
    class: className,
    ...rest
  }: Props = $props();

  function click(event: MouseEvent & { currentTarget: EventTarget & HTMLButtonElement }): void {
    if (loading) {
      event.preventDefault();
      return;
    }
    onclick?.(event);
  }
</script>

{#snippet content()}
  {#if loading}
    <Spinner />
  {:else if Icon}
    <Icon aria-hidden="true" />
  {/if}
  {#if children}<span class="label">{@render children()}</span>{/if}
{/snippet}

{#if href}
  <!-- Callers pass a resolved path (`resolve()` from $app/paths), so the link is checked there. -->
  <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
  <a {href} {download} {target} {rel} class={['button', variant, size, className]}>
    {@render content()}
  </a>
{:else}
  <button
    {type}
    {disabled}
    class={['button', variant, size, className]}
    aria-busy={loading || undefined}
    aria-disabled={loading || undefined}
    onclick={click}
    {...rest}
  >
    {@render content()}
  </button>
{/if}

<style>
  .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1_5);
    height: var(--control-md);
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    line-height: 1;
    white-space: nowrap;
    text-decoration: none;
    cursor: pointer;
    user-select: none;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out);
  }

  .button :global(svg) {
    width: var(--icon-md);
    height: var(--icon-md);
  }

  .sm {
    height: var(--control-sm);
    padding: 0 var(--space-2);
    font-size: var(--text-sm);
  }

  .lg {
    height: var(--control-lg);
    padding: 0 var(--space-4);
  }

  .primary {
    background: var(--color-ink);
    color: var(--color-on-ink);
  }

  .primary:hover {
    background: var(--color-ink-hover);
  }

  .secondary {
    background: var(--color-surface);
    color: var(--color-text);
    border-color: var(--color-border-control);
  }

  .secondary:hover {
    background: var(--color-hover);
  }

  .ghost {
    background: transparent;
    color: var(--color-text);
  }

  .ghost:hover {
    background: var(--color-hover);
  }

  .danger {
    background: var(--color-danger-solid);
    color: var(--color-on-danger);
  }

  .danger:hover {
    background: var(--color-danger-solid-hover);
  }

  /* A toggle button that is on: a filled background and a check-like border, not color alone. */
  .secondary[aria-pressed='true'] {
    border-color: var(--color-ink);
    background: var(--color-selected);
    font-weight: var(--weight-medium);
  }

  .button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .button[aria-busy='true'] {
    cursor: progress;
  }

  @media (forced-colors: active) {
    .button {
      border-color: ButtonText;
    }
  }
</style>
