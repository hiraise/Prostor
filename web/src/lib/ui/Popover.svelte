<script lang="ts" module>
  /** Props the trigger button must spread: ARIA state, the popover link and the anchor. */
  export type PopoverTriggerProps = Record<string | symbol, unknown>;
</script>

<script lang="ts">
  // A non-modal popover on the Popover API, next to the button that opens it (inline editors,
  // pickers). Focus moves into it on open and back to the button on close; Escape and a click
  // outside close it (the browser's light dismiss). On phones it is a bottom sheet (`Dialog`),
  // as every overlay there (D-019). The content is rendered only while it is open.
  import { tick, untrack, type Snippet } from 'svelte';
  import { createAttachmentKey } from 'svelte/attachments';
  import { MediaQuery } from 'svelte/reactivity';

  import { NARROW } from '#lib/theme/breakpoints.ts';

  import Dialog from './Dialog.svelte';
  import { placeElement, type Placement } from './position.ts';
  import { toasts } from './toast.svelte.ts';

  interface Props {
    /** Accessible name; the sheet's title on phones. */
    label: string;
    trigger: Snippet<[PopoverTriggerProps]>;
    /** The content; `close` closes the popover and gives focus back to the button. */
    children: Snippet<[{ close: () => void }]>;
    placement?: Placement;
    open?: boolean;
    onclose?: () => void;
  }

  let {
    label,
    trigger,
    children,
    placement = { side: 'bottom', align: 'start' },
    open = $bindable(false),
    onclose,
  }: Props = $props();

  const id = $props.id();
  const narrow = new MediaQuery(NARROW, false);
  let panel: HTMLDivElement | undefined = $state();
  let anchor: HTMLElement | null = null;

  const FOCUSABLE =
    'input:not([disabled]), textarea:not([disabled]), select:not([disabled]), button:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

  function close(): void {
    if (panel?.matches(':popover-open')) panel.hidePopover();
    else if (open) {
      open = false;
      onclose?.();
    }
  }

  function place(): void {
    if (panel && anchor) placeElement(panel, anchor, placement, 4);
  }

  // Content that grows (options arriving) is placed again, so it stays in the viewport.
  $effect(() => {
    const element = panel;
    if (!open || narrow.current || !element) return;
    const observer = new ResizeObserver(() => {
      if (element.dataset.placed !== undefined) place();
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  });

  // `open = true` from outside shows it (the field panel opens an editor by its row).
  // Toasts would cover the content, the bottom sheet on phones above all (backlog 1.4c).
  $effect(() => {
    if (open) untrack(() => toasts.dismissAll());
  });

  $effect(() => {
    if (!open || narrow.current || !panel || panel.matches(':popover-open')) return;
    panel.showPopover();
  });

  function handleToggle(event: ToggleEvent): void {
    const element = panel;
    if (!element) return;
    if (event.newState === 'open') {
      open = true;
      // The content renders on `open`: place and focus it once it is there.
      void tick().then(() => {
        if (!element.matches(':popover-open')) return;
        place();
        element.dataset.placed = '';
        const target =
          element.querySelector<HTMLElement>('[autofocus]') ??
          element.querySelector<HTMLElement>(FOCUSABLE) ??
          element;
        target.focus();
      });
    } else {
      delete element.dataset.placed;
      open = false;
      const focused = document.activeElement;
      if (!focused || focused === document.body || element.contains(focused)) anchor?.focus();
      onclose?.();
    }
  }

  const anchorKey = createAttachmentKey();
  function attachAnchor(element: HTMLElement): () => void {
    anchor = element;
    return () => {
      if (anchor === element) anchor = null;
    };
  }

  const triggerProps: PopoverTriggerProps = $derived(
    narrow.current
      ? {
          'aria-haspopup': 'dialog',
          'aria-expanded': open,
          onclick: () => {
            open = true;
          },
          [anchorKey]: attachAnchor,
        }
      : {
          popovertarget: `${id}-popover`,
          'aria-haspopup': 'dialog',
          'aria-expanded': open,
          'aria-controls': `${id}-popover`,
          [anchorKey]: attachAnchor,
        },
  );
</script>

{@render trigger(triggerProps)}

{#if narrow.current}
  <Dialog bind:open title={label} {onclose}>
    {#if open}{@render children({ close })}{/if}
  </Dialog>
{:else}
  <div
    bind:this={panel}
    id="{id}-popover"
    class="popover"
    role="dialog"
    aria-label={label}
    popover="auto"
    tabindex="-1"
    ontoggle={handleToggle}
  >
    {#if open}{@render children({ close })}{/if}
  </div>
{/if}

<style>
  .popover {
    position: fixed;
    inset: auto;
    width: max-content;
    min-width: 16rem;
    max-width: min(24rem, calc(100vw - 1rem));
    max-height: calc(100dvh - 1rem);
    margin: 0;
    padding: var(--space-3);
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
    color: var(--color-text);
    box-shadow: var(--shadow-md);
  }

  /* Hidden until positioned, so it never flashes at the wrong place. */
  .popover:not([data-placed]) {
    visibility: hidden;
  }

  @media (forced-colors: active) {
    .popover {
      border: 1px solid CanvasText;
    }
  }
</style>
