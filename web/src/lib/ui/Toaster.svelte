<script lang="ts">
  // Shows the toasts of `toasts` (UI-038); rendered once by the root layout. Screen readers hear
  // each toast through `announce`, so the list itself is not a live region. On phones the stack
  // spans the width above the home indicator (D-019).
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Info from '@lucide/svelte/icons/info';
  import X from '@lucide/svelte/icons/x';

  import * as common from '#lib/i18n/messages/common.ts';

  import Button from './Button.svelte';
  import IconButton from './IconButton.svelte';
  import { toasts } from './toast.svelte.ts';

  const icons = { info: Info, success: CircleCheck, error: CircleAlert };
  let region: HTMLElement | undefined = $state();

  function focusLeft(event: FocusEvent): void {
    if (!region?.contains(event.relatedTarget as Node | null)) toasts.resume();
  }
</script>

{#if toasts.items.length > 0}
  <section
    bind:this={region}
    class="toaster"
    aria-label={common.statusMessages()}
    onpointerenter={() => {
      toasts.pause();
    }}
    onpointerleave={() => {
      toasts.resume();
    }}
    onfocusin={() => {
      toasts.pause();
    }}
    onfocusout={focusLeft}
  >
    <ol>
      {#each toasts.items as toast (toast.id)}
        {@const Icon = icons[toast.tone]}
        <li class={['toast', toast.tone]}>
          <span class="icon"><Icon aria-hidden="true" /></span>
          <div class="text">
            <p>{toast.message}</p>
            {#if toast.detail}<p class="detail">{toast.detail}</p>{/if}
          </div>
          <div class="actions">
            {#if toast.undo}
              <Button size="sm" variant="ghost" onclick={() => toasts.undo(toast.id)}>
                {common.undo()}
              </Button>
            {/if}
            {#if toast.action}
              <Button size="sm" variant="ghost" onclick={() => toasts.act(toast.id)}>
                {toast.action.label}
              </Button>
            {/if}
            {#if toast.retry}
              <Button size="sm" variant="ghost" onclick={() => toasts.retry(toast.id)}>
                {common.retry()}
              </Button>
            {/if}
            <IconButton
              icon={X}
              size="sm"
              label={common.dismiss()}
              onclick={() => {
                toasts.dismiss(toast.id);
              }}
            />
          </div>
        </li>
      {/each}
    </ol>
  </section>
{/if}

<style>
  .toaster {
    position: fixed;
    z-index: var(--z-toast);
    inset-inline-end: calc(var(--space-4) + var(--safe-right));
    inset-block-end: calc(var(--space-4) + var(--safe-bottom));
    width: min(26rem, calc(100vw - 2 * var(--space-4)));
  }

  ol {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-inline-start: 3px solid var(--color-info);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-md);
    animation: rise var(--duration-normal) var(--ease-out);
  }

  .icon {
    display: inline-flex;
    padding-top: 0.45em;
    color: var(--color-info);
  }

  .icon :global(svg) {
    width: var(--icon-md);
    height: var(--icon-md);
  }

  .success {
    border-inline-start-color: var(--color-success);
  }

  .success .icon {
    color: var(--color-success);
  }

  .error {
    border-inline-start-color: var(--color-danger);
  }

  .error .icon {
    color: var(--color-danger);
  }

  .text {
    flex: 1;
    min-width: 0;
    padding-block: var(--space-1);
    overflow-wrap: anywhere;
  }

  .detail {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    user-select: all;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  @media (width < 48rem) {
    .toaster {
      inset-inline: calc(var(--space-2) + var(--safe-left)) calc(var(--space-2) + var(--safe-right));
      inset-block-end: calc(var(--space-2) + var(--safe-bottom));
      width: auto;
    }
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(0.5rem);
    }
  }
</style>
