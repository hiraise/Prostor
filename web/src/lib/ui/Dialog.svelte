<script lang="ts">
  // A modal dialog on the native <dialog> element: the browser makes the page behind it inert and
  // keeps focus inside (UI-032). On narrow screens it becomes a bottom sheet (D-019); `side="start"`
  // makes it a drawer from the leading edge (navigation on phones), `side="end"` a side panel from
  // the trailing edge (settings) that is a tall bottom sheet on a phone.
  //
  // Focus: on open it goes to the element with `autofocus`, else the first control of the body,
  // else the close button; on close it returns to the element that opened the dialog.
  import X from '@lucide/svelte/icons/x';
  import type { Snippet } from 'svelte';

  import * as common from '#lib/i18n/messages/common.ts';

  import IconButton from './IconButton.svelte';

  interface Props {
    open?: boolean;
    title: string;
    /** Keeps the title for screen readers only (a drawer whose content is self-explanatory). */
    hideTitle?: boolean;
    description?: string;
    side?: 'center' | 'start' | 'end';
    size?: 'sm' | 'md' | 'lg' | 'xl';
    class?: string;
    /** Escape, the backdrop and the close button close it; false while something must finish. */
    dismissible?: boolean;
    /**
     * No title row and no close button of its own: the content carries them (the board's issue
     * popup puts its corner controls into the issue's own metadata line). The title stays for
     * screen readers; Escape and the backdrop still close.
     */
    bare?: boolean;
    children: Snippet;
    footer?: Snippet;
    onclose?: () => void;
  }

  let {
    open = $bindable(false),
    title,
    hideTitle = false,
    description,
    side = 'center',
    size = 'md',
    class: className,
    dismissible = true,
    bare = false,
    children,
    footer,
    onclose,
  }: Props = $props();

  const id = $props.id();
  let dialog: HTMLDialogElement | undefined = $state();
  let body: HTMLDivElement | undefined = $state();
  let opener: HTMLElement | null = null;
  let pressedOnBackdrop = false;

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function focusInitial(element: HTMLDialogElement): void {
    const target =
      element.querySelector<HTMLElement>('[autofocus]') ??
      body?.querySelector<HTMLElement>(FOCUSABLE) ??
      element.querySelector<HTMLElement>(FOCUSABLE);
    target?.focus();
  }

  $effect(() => {
    const element = dialog;
    if (!element) return;
    if (open && !element.open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      element.showModal();
      document.documentElement.classList.add('tf-modal-open');
      focusInitial(element);
    } else if (!open && element.open) {
      element.close();
    }
  });

  // Closed by the browser (Escape), by the close button or by `open = false`.
  function handleClose(): void {
    open = false;
    if (!document.querySelector('dialog[open]')) {
      document.documentElement.classList.remove('tf-modal-open');
    }
    if (opener?.isConnected) opener.focus();
    opener = null;
    onclose?.();
  }

  function handleCancel(event: Event): void {
    if (!dismissible) event.preventDefault();
  }

  // A click that starts and ends on the backdrop closes the dialog; a text selection dragged out
  // of the dialog does not.
  function handlePointerDown(event: PointerEvent): void {
    pressedOnBackdrop = event.target === dialog;
  }

  function handleClick(event: MouseEvent): void {
    if (dismissible && pressedOnBackdrop && event.target === dialog) open = false;
    pressedOnBackdrop = false;
  }
</script>

<dialog
  bind:this={dialog}
  class={['dialog', side, size, className]}
  aria-labelledby="{id}-title"
  aria-describedby={description ? `${id}-description` : undefined}
  onclose={handleClose}
  oncancel={handleCancel}
  onpointerdown={handlePointerDown}
  onclick={handleClick}
>
  <div class="panel">
    {#if bare}
      <h2 id="{id}-title" class="sr-only">{title}</h2>
    {:else}
      <header class={{ 'title-hidden': hideTitle }}>
        <h2 id="{id}-title" class={{ 'sr-only': hideTitle }}>{title}</h2>
        {#if dismissible}
          <IconButton
            icon={X}
            label={common.close()}
            onclick={() => {
              open = false;
            }}
          />
        {/if}
      </header>
    {/if}
    {#if description}<p id="{id}-description" class="description">{description}</p>{/if}
    <div class="body" bind:this={body}>
      {@render children()}
    </div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</dialog>

<style>
  :global(html.tf-modal-open) {
    overflow: hidden;
  }

  .dialog {
    width: min(32rem, calc(100vw - 2 * var(--space-4)));
    max-width: none;
    max-height: min(48rem, calc(100dvh - 2 * var(--space-8)));
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
    color: var(--color-text);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
  }

  .dialog.sm {
    width: min(24rem, calc(100vw - 2 * var(--space-4)));
  }

  .dialog.lg {
    width: min(48rem, calc(100vw - 2 * var(--space-4)));
  }

  /* Issue forms and the board's popup: the two-column core needs the room. */
  .dialog.xl {
    width: min(73.75rem, calc(100vw - 2 * var(--space-4)));
  }

  .dialog[open] {
    animation: appear var(--duration-normal) var(--ease-out);
  }

  .dialog::backdrop {
    background: var(--color-backdrop);
  }

  .panel {
    display: flex;
    flex-direction: column;
    max-height: inherit;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-3) var(--space-2) var(--space-5);
  }

  header.title-hidden {
    justify-content: flex-end;
  }

  h2 {
    font-size: var(--text-lg);
  }

  .description {
    padding: 0 var(--space-5);
    color: var(--color-text-muted);
  }

  .body {
    flex: 1;
    min-height: 0;
    padding: var(--space-3) var(--space-5) var(--space-5);
    overflow: auto;
    overscroll-behavior: contain;
  }

  .dialog.new-issue .panel {
    height: min(44rem, calc(100dvh - 2 * var(--space-8)));
  }

  .dialog.new-issue .body {
    display: flex;
    flex-direction: column;
    padding: 0;
  }

  footer {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-5);
    border-top: 1px solid var(--color-border);
  }

  /* A drawer from the leading edge, full height. */
  .dialog.start {
    width: min(20rem, calc(100vw - var(--space-12)));
    height: 100dvh;
    max-height: none;
    margin: 0;
    margin-inline-end: auto;
    border-width: 0 1px 0 0;
    border-radius: 0;
    background: var(--color-surface-sunken);
    animation-name: slide-in;
  }

  .dialog.start .panel {
    height: 100%;
    padding: var(--safe-top) 0 var(--safe-bottom) var(--safe-left);
  }

  .dialog.start header {
    padding: var(--space-2);
  }

  .dialog.start .body {
    padding: 0;
  }

  /* A side panel from the trailing edge, full height (settings beside the content). */
  .dialog.end {
    width: min(32rem, 100vw);
    height: 100dvh;
    max-height: none;
    margin: 0;
    margin-inline-start: auto;
    border-width: 0 0 0 1px;
    border-radius: 0;
    animation-name: slide-in-end;
  }

  .dialog.end .panel {
    height: 100%;
    padding: var(--safe-top) var(--safe-right) var(--safe-bottom) 0;
  }

  /* Phones: a bottom sheet, thumb-reachable, clear of the home indicator. */
  @media (width < 48rem) {
    .dialog.center {
      width: 100%;
      max-width: 100%;
      max-height: calc(100dvh - var(--safe-top) - var(--space-8));
      margin: auto 0 0;
      border-width: 1px 0 0;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      animation-name: slide-up;
    }

    .dialog.center .panel {
      padding: 0 var(--safe-right) var(--safe-bottom) var(--safe-left);
    }

    .dialog.end {
      width: 100%;
      max-width: 100%;
      height: calc(100dvh - var(--safe-top) - var(--space-8));
      margin: auto 0 0;
      border-width: 1px 0 0;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      animation-name: slide-up;
    }

    .dialog.end .panel {
      padding: 0 var(--safe-right) var(--safe-bottom) var(--safe-left);
    }

    /* A grip shows that this is a sheet. */
    .dialog.center .panel::before,
    .dialog.end .panel::before {
      content: '';
      align-self: center;
      width: 2.5rem;
      height: 0.25rem;
      margin-top: var(--space-2);
      border-radius: var(--radius-full);
      background: var(--color-border-control);
    }

    .dialog.issue-popup.center {
      height: 100dvh;
      max-height: 100dvh;
      border: 0;
      border-radius: 0;
    }

    .dialog.issue-popup .panel {
      height: 100%;
      padding-top: var(--safe-top);
    }

    .dialog.issue-popup.center .panel::before {
      display: none;
    }

    footer {
      flex-direction: column-reverse;
    }

    footer > :global(*) {
      width: 100%;
    }
  }

  @keyframes appear {
    from {
      opacity: 0;
      transform: translateY(0.5rem) scale(0.98);
    }
  }

  @keyframes slide-up {
    from {
      transform: translateY(100%);
    }
  }

  @keyframes slide-in-end {
    from {
      transform: translateX(100%);
    }
  }

  @keyframes slide-in {
    from {
      transform: translateX(-100%);
    }
  }

  @media (forced-colors: active) {
    .dialog {
      border: 1px solid CanvasText;
    }
  }
</style>
