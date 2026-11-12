<script lang="ts">
  // An inline message across a page or a form: information, a warning, a problem or a success.
  // The tone has its own icon as well as its color (UI-032).
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Info from '@lucide/svelte/icons/info';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import X from '@lucide/svelte/icons/x';
  import type { Snippet } from 'svelte';

  import * as common from '#lib/i18n/messages/common.ts';

  import IconButton from './IconButton.svelte';

  interface Props {
    tone?: 'info' | 'success' | 'warning' | 'danger';
    children: Snippet;
    ondismiss?: () => void;
  }

  let { tone = 'info', children, ondismiss }: Props = $props();

  const icons = { info: Info, success: CircleCheck, warning: TriangleAlert, danger: CircleAlert };
  const Icon = $derived(icons[tone]);
</script>

<div class={['notice', tone]}>
  <span class="icon"><Icon aria-hidden="true" /></span>
  <div class="text">{@render children()}</div>
  {#if ondismiss}
    <IconButton icon={X} label={common.dismiss()} size="sm" onclick={ondismiss} />
  {/if}
</div>

<style>
  .notice {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-info-bg);
    color: var(--color-text);
  }

  .icon {
    display: inline-flex;
    padding-top: 0.15em;
    color: var(--color-info);
  }

  .icon :global(svg) {
    width: var(--icon-md);
    height: var(--icon-md);
  }

  .text {
    flex: 1;
    min-width: 0;
    padding-block: 0.1em;
    align-self: center;
  }

  .success {
    background: var(--color-success-bg);
  }

  .success .icon {
    color: var(--color-success);
  }

  .warning {
    background: var(--color-warning-bg);
  }

  .warning .icon {
    color: var(--color-warning);
  }

  .danger {
    background: var(--color-danger-bg);
  }

  .danger .icon {
    color: var(--color-danger);
  }

  @media (forced-colors: active) {
    .notice {
      border: 1px solid CanvasText;
    }
  }
</style>
