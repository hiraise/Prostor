<script lang="ts" module>
  // The menu's types live in `menu.ts`, beside the component, so plain modules can import them
  // too; everything they were is re-exported unchanged.
  import type { MenuAction, MenuEntry, MenuRadio, MenuTriggerProps } from './menu.ts';

  export type * from './menu.ts';
</script>

<script lang="ts">
  // A menu button (WAI-ARIA menu pattern) on the Popover API: the browser puts the menu in the top
  // layer and closes it on outside clicks and Escape. Keyboard: Enter, Space or ArrowDown opens on
  // the first item, ArrowUp on the last; arrows, Home and End move; typing jumps to an item;
  // Escape and Tab close; focus returns to the trigger.
  import Check from '@lucide/svelte/icons/check';
  import type { Snippet } from 'svelte';
  import { createAttachmentKey } from 'svelte/attachments';

  import { placeElement, type Placement } from './position.ts';
  import { toasts } from './toast.svelte.ts';

  interface Props {
    /** Accessible name of the menu. */
    label: string;
    items: MenuEntry[];
    trigger: Snippet<[MenuTriggerProps]>;
    placement?: Placement;
  }

  let { label, items, trigger, placement = { side: 'bottom', align: 'start' } }: Props = $props();

  const id = $props.id();
  let menu: HTMLDivElement | undefined = $state();
  let anchor: HTMLElement | null = null;
  let open = $state(false);
  // The entries as of opening. A late-arriving entry (the clone items of the issue page, loaded
  // per project) joins at the end while the menu is open: it must not slide the shown entries
  // under a pointer or a screen reader's focus. The next opening shows the natural order.
  let opened: MenuEntry[] = $state([]);
  let focusOnOpen: 'first' | 'last' = 'first';
  let typed = '';
  let typedTimer: ReturnType<typeof setTimeout> | undefined;

  function enabledItems(): HTMLElement[] {
    return menu
      ? [...menu.querySelectorAll<HTMLElement>('[role^="menuitem"]:not([aria-disabled="true"])')]
      : [];
  }

  function focusItem(which: 'first' | 'last' | number): void {
    const list = enabledItems();
    if (list.length === 0) return;
    const index =
      which === 'first'
        ? 0
        : which === 'last'
          ? list.length - 1
          : (which + list.length) % list.length;
    list[index]?.focus();
  }

  function show(where: 'first' | 'last'): void {
    focusOnOpen = where;
    if (menu && !menu.matches(':popover-open')) menu.showPopover();
  }

  function hide(): void {
    if (menu?.matches(':popover-open')) menu.hidePopover();
  }

  function keyOf(entry: MenuEntry): string | null {
    // Separators have no identity; a late one is cosmetic and skipped.
    if (entry.type === 'separator') return null;
    return entry.type === 'group' ? `group:${entry.label}` : `${entry.type}:${entry.label}`;
  }

  // While open: the opening entries, then the entries that arrived since, in their own order.
  const shown = $derived.by(() => {
    if (!open) return items;
    const seen = new Set(opened.map(keyOf).filter((key): key is string => key !== null));
    const late = items.filter((entry) => {
      const key = keyOf(entry);
      return key !== null && !seen.has(key);
    });
    return late.length > 0 ? [...opened, ...late] : opened;
  });

  function handleToggle(event: ToggleEvent): void {
    open = event.newState === 'open';
    if (!menu) return;
    if (open) {
      opened = items;
      toasts.dismissAll();
      if (anchor) placeElement(menu, anchor, placement, 4);
      menu.dataset.placed = '';
      focusItem(focusOnOpen);
      focusOnOpen = 'first';
    } else {
      delete menu.dataset.placed;
      opened = [];
      // Closed by Escape, a selection or an outside click: give focus back unless the user
      // already moved it somewhere else.
      const focused = document.activeElement;
      if (!focused || focused === document.body || menu.contains(focused)) anchor?.focus();
    }
  }

  function handleTriggerKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      show('first');
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      show('last');
    }
  }

  function typeahead(key: string): void {
    clearTimeout(typedTimer);
    typed += key.toLowerCase();
    typedTimer = setTimeout(() => {
      typed = '';
    }, 500);
    const list = enabledItems();
    const current = list.indexOf(document.activeElement as HTMLElement);
    // A repeated single letter cycles through the items starting with it.
    const ordered = [...list.slice(current + 1), ...list.slice(0, current + 1)];
    const search = typed.length > 1 && new Set(typed).size === 1 ? (typed[0] ?? '') : typed;
    ordered.find((item) => item.textContent.trim().toLowerCase().startsWith(search))?.focus();
  }

  function handleMenuKeydown(event: KeyboardEvent): void {
    const list = enabledItems();
    const current = list.indexOf(document.activeElement as HTMLElement);
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        focusItem(current + 1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        focusItem(current - 1);
        break;
      case 'Home':
        event.preventDefault();
        focusItem('first');
        break;
      case 'End':
        event.preventDefault();
        focusItem('last');
        break;
      case 'Tab':
        hide();
        break;
      default:
        if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
          typeahead(event.key);
        }
    }
  }

  function select(item: MenuAction | MenuRadio): void {
    if (item.type === 'action' && item.disabled) return;
    hide();
    item.onSelect();
  }

  // Remembers the trigger element, which positions the menu and gets focus back.
  const anchorKey = createAttachmentKey();
  function attachAnchor(element: HTMLElement): () => void {
    anchor = element;
    return () => {
      if (anchor === element) anchor = null;
    };
  }

  const triggerProps: MenuTriggerProps = $derived({
    popovertarget: `${id}-menu`,
    'aria-haspopup': 'menu',
    'aria-expanded': open,
    'aria-controls': `${id}-menu`,
    onkeydown: handleTriggerKeydown,
    [anchorKey]: attachAnchor,
  });
</script>

{#snippet entry(item: MenuAction | MenuRadio)}
  {#if item.type === 'radio'}
    <button
      type="button"
      role="menuitemradio"
      tabindex="-1"
      class="item"
      aria-checked={item.checked}
      lang={item.lang}
      onclick={() => {
        select(item);
      }}
    >
      <span class="mark"
        >{#if item.checked}<Check aria-hidden="true" />{/if}</span
      >
      {item.label}
    </button>
  {:else}
    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      class={['item', { danger: item.danger }]}
      aria-disabled={item.disabled ? true : undefined}
      onclick={() => {
        select(item);
      }}
    >
      <span class="mark"
        >{#if item.icon}<item.icon aria-hidden="true" />{/if}</span
      >
      {item.label}
    </button>
  {/if}
{/snippet}

{@render trigger(triggerProps)}

<div
  bind:this={menu}
  id="{id}-menu"
  class="menu"
  role="menu"
  aria-label={label}
  popover="auto"
  tabindex="-1"
  ontoggle={handleToggle}
  onkeydown={handleMenuKeydown}
>
  {#each shown as item, index (index)}
    {#if item.type === 'separator'}
      <div class="separator" role="separator"></div>
    {:else if item.type === 'group'}
      <div class="group" role="group" aria-labelledby="{id}-group-{index}">
        <div id="{id}-group-{index}" class="group-label">{item.label}</div>
        {#each item.items as child, childIndex (childIndex)}
          {@render entry(child)}
        {/each}
      </div>
    {:else}
      {@render entry(item)}
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    inset: auto;
    margin: 0;
    min-width: 12rem;
    max-width: min(20rem, calc(100vw - 1rem));
    max-height: calc(100dvh - 1rem);
    padding: var(--space-1);
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
    color: var(--color-text);
    box-shadow: var(--shadow-md);
  }

  /* Hidden until positioned, so it never flashes at the wrong place. */
  .menu:not([data-placed]) {
    visibility: hidden;
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--control-md);
    padding: 0 var(--space-3) 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    font-size: var(--text-md);
    text-align: start;
    cursor: pointer;
  }

  .item:hover,
  .item:focus {
    background: var(--color-hover);
  }

  .item:focus-visible {
    outline-offset: -2px;
  }

  .item[aria-disabled='true'] {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .item.danger {
    color: var(--color-danger);
  }

  .mark {
    display: inline-flex;
    width: var(--icon-md);
    color: var(--color-text-muted);
  }

  .mark :global(svg) {
    width: var(--icon-md);
    height: var(--icon-md);
  }

  [aria-checked='true'] .mark {
    color: var(--color-text);
  }

  .group + .group,
  .item + .group,
  .group + .item {
    margin-top: var(--space-1);
  }

  .group-label {
    padding: var(--space-1) var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
  }

  .separator {
    height: 1px;
    margin: var(--space-1) 0;
    background: var(--color-border);
  }

  @media (forced-colors: active) {
    .menu {
      border: 1px solid CanvasText;
    }
  }
</style>
