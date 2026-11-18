<script lang="ts">
  // The application frame (UI-001, UI-033): on wide screens a navigation rail that collapses to
  // icons and, expanded, resizes by its edge (D-058); on phones a top bar whose menu button opens
  // the navigation as a drawer (D-019). The account menu (UI-003) sits at the bottom of the rail
  // and at the end of the top bar. The skip link and the single <main> landmark come first for
  // keyboard and screen reader users.
  import Menu from '@lucide/svelte/icons/menu';
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close';
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open';
  import SquareKanban from '@lucide/svelte/icons/square-kanban';
  import type { Snippet } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';
  import { goto } from '$app/navigation';

  import {
    boardMessages,
    canCreateBoards,
    NewBoardHost,
    openNewBoard,
  } from '#lib/features/boards/index.ts';
  import { CreateMenu, issueCreateExtra, NewIssueHost } from '#lib/features/issues/index.ts';
  import { NotificationBell } from '#lib/features/notifications/index.ts';
  import { createMatchState, match } from '#lib/shortcuts/match.ts';
  import {
    active,
    dispatchClaim,
    focusQuery,
    focusedEntity,
  } from '#lib/shortcuts/registry.svelte.ts';
  import { tables } from '#lib/shortcuts/tables/index.ts';
  import { isTypingFocus } from '#lib/shortcuts/typing.ts';
  import { NARROW } from '#lib/theme/breakpoints.ts';
  import { copyLink, copyText } from '#lib/ui/copy.ts';
  import Dialog from '#lib/ui/Dialog.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import type { MenuEntry } from '#lib/ui/menu.ts';
  import Notice from '#lib/ui/Notice.svelte';
  import Resizer from '#lib/ui/Resizer.svelte';

  import { currentUser, preferences } from '#lib/session/index.ts';

  import AccountMenu from './AccountMenu.svelte';
  import { missingFeatures } from './browser-support.ts';
  import Logo from './Logo.svelte';
  import * as m from './messages.ts';
  import {
    isRailCollapsed,
    RAIL_WIDTH_MAX,
    RAIL_WIDTH_MIN,
    railWidth,
    setRailCollapsed,
    setRailWidth,
  } from './navigation.svelte.ts';
  import NavList from './NavList.svelte';
  import PreferencesMenu from './PreferencesMenu.svelte';
  import { homeHref, visibleSections } from './sections.ts';

  interface Props {
    pathname: string;
    children: Snippet;
  }

  let { pathname, children }: Props = $props();

  const narrow = new MediaQuery(NARROW, false);
  const user = $derived(currentUser());
  /** «Создать» entries of other features (UI-002): the open issue's subtask and linked issue
   * (step 2.2 §7), then a board. */
  const boardEntry = $derived<MenuEntry | null>(
    canCreateBoards()
      ? {
          type: 'action',
          label: boardMessages.createBoardItem(),
          icon: SquareKanban,
          onSelect: () => {
            openNewBoard();
          },
        }
      : null,
  );
  const createMore = $derived<MenuEntry[]>([
    ...issueCreateExtra(),
    ...(boardEntry !== null ? [boardEntry] : []),
  ]);
  const collapsed = $derived(isRailCollapsed());
  let drawerOpen = $state(false);
  let oldBrowser = $state(missingFeatures().length > 0);
  /** The width being dragged, before it is kept (rem × 10). */
  let dragged = $state<number | null>(null);
  const width = $derived(dragged ?? railWidth());

  function resize(value: number, done: boolean): void {
    if (!done) {
      dragged = value;
      return;
    }
    dragged = null;
    setRailWidth(value);
  }

  // A drawer left open on a phone must not survive turning it into a wide screen.
  $effect(() => {
    if (!narrow.current) drawerOpen = false;
  });

  function closeDrawer(): void {
    drawerOpen = false;
  }

  // ---- The global shortcuts (step 2.2 §4) ----

  // The one keydown listener of the `(app)` layout (§3): nobody else listens for app shortcuts.
  // A modal dialog or an open popover (a menu) owns the keys; a page's own earlier handler that
  // already took the event (defaultPrevented) keeps it.
  const matchState = createMatchState(isTypingFocus);

  function onKeydown(event: KeyboardEvent): void {
    if (event.defaultPrevented) return;
    if (document.querySelector('dialog[open], :popover-open')) return;
    const binding = match(event, tables, active(), matchState);
    if (binding === null) {
      // A sequence's leading key is not a stray letter (§3): while one is armed — or this press
      // armed one — the next key completes it instead of opening the command dialog.
      if (matchState.armed === null && typedCommand(event)) event.preventDefault();
      return;
    }
    if (run(binding.id)) event.preventDefault();
  }

  /** `commands_on_typing` (2.1, D-063): a bare letter or digit no binding owns opens the
   * command dialog prefilled with it — the page that claims the `command` binding owns the
   * dialog, and its answer says whether the letter was taken. */
  function typedCommand(event: KeyboardEvent): boolean {
    if (!preferences.value('commands_on_typing')) return false;
    if (event.metaKey || event.ctrlKey || event.altKey) return false;
    if (matchState.isTyping()) return false;
    if (event.key.length !== 1 || !/\p{L}|\p{Nd}/u.test(event.key)) return false;
    return dispatchClaim('command', event.key);
  }

  /** Runs a global binding; false lets the key fall through untouched. */
  function run(id: string): boolean {
    switch (id) {
      case 'overlay':
        openOverlay();
        return true;
      case 'focus-query':
        // The page's query box when there is one, else «Задачи» (§4).
        if (focusQuery()) return true;
        // The address comes resolved from sections.ts.
        // eslint-disable-next-line svelte/no-navigation-without-resolve
        void goto(sectionHref('issues'));
        return true;
      case 'create': {
        // The «Создать» menu of the rail or the top bar, wherever it shows.
        const trigger = document.querySelector<HTMLButtonElement>('[data-create-menu] button');
        trigger?.click();
        return trigger !== null;
      }
      case 'copy.id':
        return copyEntity('id');
      case 'copy.link':
        return copyEntity('link');
      case 'copy.full':
        return copyEntity('full');
      case 'rail':
        setRailCollapsed(!isRailCollapsed());
        return true;
      case 'go.issues':
        return goTo('issues');
      case 'go.boards':
        return goTo('boards');
      case 'go.projects':
        return goTo('projects');
      case 'go.admin':
        return goTo('admin');
      case 'command':
      case 'command.repeat':
        // The page with the open or selected items owns the dialog (§4); parts 2–3 claim it.
        return dispatchClaim(id);
      default:
        return false;
    }
  }

  function goTo(id: 'issues' | 'boards' | 'projects' | 'admin'): boolean {
    const section = visibleSections().find((entry) => entry.id === id);
    if (!section) return false;
    // The address comes resolved from sections.ts.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(section.href);
    return true;
  }

  function sectionHref(id: 'issues' | 'boards' | 'projects' | 'admin'): string {
    return visibleSections().find((entry) => entry.id === id)?.href ?? homeHref();
  }

  /** The copy trio (§4) of the focused entity; false when the page has none in focus. */
  function copyEntity(what: 'id' | 'link' | 'full'): boolean {
    const entity = focusedEntity();
    if (entity === null) return false;
    if (what === 'link') void copyLink(entity.href);
    else void copyText(what === 'id' ? entity.id : `${entity.id} ${entity.summary}`);
    return true;
  }

  // The reference overlay (UI-020), a lazy import: first use pays (§5).
  let overlayWanted = $state(false);
  let overlayOpen = $state(false);

  function openOverlay(): void {
    overlayWanted = true;
    overlayOpen = true;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- Links here lead home: homeHref() resolves the route (sections.ts). -->
<!-- eslint-disable svelte/no-navigation-without-resolve -->
<a class="skip" href="#main">{m.skipToContent()}</a>

<div
  class={['shell', { narrow: narrow.current, collapsed: collapsed && !narrow.current }]}
  style:--rail-expanded="{width / 10}rem"
>
  {#if narrow.current}
    <header class="topbar">
      <IconButton
        icon={Menu}
        label={m.mainNavigation()}
        aria-haspopup="dialog"
        aria-expanded={drawerOpen}
        onclick={() => {
          drawerOpen = true;
        }}
      />
      <a class="home" href={homeHref()} aria-label={m.homeLink()}><Logo /></a>
      <div class="spacer"></div>
      {#if user}<NotificationBell {user} placement={{ side: 'bottom', align: 'end' }} />{/if}
      {#if user}<span class="create-host" data-create-menu
          ><CreateMenu
            compact
            placement={{ side: 'bottom', align: 'end' }}
            more={createMore}
          /></span
        >{/if}
      {#if user}
        <AccountMenu {user} placement={{ side: 'bottom', align: 'end' }} onhelp={openOverlay} />
      {:else}
        <PreferencesMenu placement={{ side: 'bottom', align: 'end' }} />
      {/if}
    </header>

    <Dialog bind:open={drawerOpen} title={m.mainNavigation()} hideTitle side="start">
      <nav aria-label={m.mainNavigation()} class="drawer-nav">
        <a
          class="home drawer-home"
          href={homeHref()}
          aria-label={m.homeLink()}
          onclick={closeDrawer}
        >
          <Logo />
        </a>
        <NavList {pathname} onnavigate={closeDrawer} />
      </nav>
    </Dialog>
  {:else}
    <div class="rail-frame">
      <nav class="rail" aria-label={m.mainNavigation()}>
        <a class="home rail-home" href={homeHref()} aria-label={m.homeLink()}>
          <Logo compact={collapsed} />
        </a>
        {#if user}
          <div class="rail-create" data-create-menu>
            <CreateMenu
              more={createMore}
              compact={collapsed}
              placement={{ side: collapsed ? 'right' : 'bottom', align: 'start' }}
            />
          </div>
        {/if}
        <NavList {pathname} {collapsed} />
        <div class="rail-footer">
          {#if user}
            <NotificationBell
              {user}
              placement={{
                side: collapsed ? 'right' : 'top',
                align: collapsed ? 'end' : 'start',
              }}
            />
            <AccountMenu
              {user}
              withName={!collapsed}
              placement={{ side: collapsed ? 'right' : 'top', align: collapsed ? 'end' : 'start' }}
              onhelp={openOverlay}
            />
          {:else}
            <PreferencesMenu placement={{ side: collapsed ? 'right' : 'top', align: 'end' }} />
          {/if}
          <IconButton
            icon={collapsed ? PanelLeftOpen : PanelLeftClose}
            label={collapsed ? m.expandNavigation() : m.collapseNavigation()}
            tooltipPlacement={{ side: 'right', align: 'center' }}
            onclick={() => {
              setRailCollapsed(!collapsed);
            }}
          />
        </div>
      </nav>
      {#if !collapsed}
        <div class="handle">
          <Resizer
            value={width}
            min={RAIL_WIDTH_MIN}
            max={RAIL_WIDTH_MAX}
            side="start"
            label={m.railWidth()}
            onchange={resize}
          />
        </div>
      {/if}
    </div>
  {/if}

  <div class="content">
    {#if oldBrowser}
      <div class="notices">
        <Notice
          tone="warning"
          ondismiss={() => {
            oldBrowser = false;
          }}
        >
          {m.oldBrowser()}
        </Notice>
      </div>
    {/if}
    <main id="main" tabindex="-1">
      {@render children()}
    </main>
  </div>
</div>

<!-- eslint-enable svelte/no-navigation-without-resolve -->

{#if user}<NewIssueHost /><NewBoardHost />{/if}

{#if overlayWanted}
  {#await import('./ShortcutsOverlay.svelte') then Overlay}
    <Overlay.default bind:open={overlayOpen} />
  {/await}
{/if}

<style>
  .skip {
    position: fixed;
    z-index: var(--z-toast);
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    inset-block-start: calc(var(--space-2) + var(--safe-top));
    inset-inline-start: calc(var(--space-2) + var(--safe-left));
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-ink);
    color: var(--color-on-ink);
    font-weight: var(--weight-medium);
    text-decoration: none;
    transform: translateY(calc(-100% - var(--space-4) - var(--safe-top)));
  }

  .skip:focus-visible {
    transform: none;
  }

  .shell {
    display: grid;
    grid-template-columns: var(--rail-expanded) minmax(0, 1fr);
    min-height: 100dvh;
  }

  .shell.collapsed {
    grid-template-columns: var(--rail-collapsed) minmax(0, 1fr);
  }

  .shell.narrow {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto 1fr;
  }

  .rail-frame {
    position: sticky;
    top: 0;
    z-index: var(--z-rail);
    display: flex;
    height: 100dvh;
  }

  .rail {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: calc(var(--space-3) + var(--safe-top)) 0 calc(var(--space-3) + var(--safe-bottom));
    padding-inline-start: var(--safe-left);
    border-inline-end: 1px solid var(--color-border);
    background: var(--color-surface-sunken);
    overflow: hidden auto;
  }

  .handle {
    display: flex;
    position: absolute;
    inset-block: 0;
    inset-inline-end: 0;
    translate: 50% 0;
    z-index: 1;
  }

  .home {
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    border-radius: var(--radius-md);
    color: inherit;
    text-decoration: none;
  }

  .rail-home {
    align-self: flex-start;
    margin: 0 var(--space-2) var(--space-3);
    padding: var(--space-1) var(--space-2);
  }

  .collapsed .rail-home {
    align-self: center;
    margin-inline: 0;
  }

  .rail-create {
    display: flex;
    padding: 0 var(--space-2) var(--space-1);
  }

  .collapsed .rail-create {
    justify-content: center;
  }

  .rail-footer {
    display: flex;
    gap: var(--space-1);
    margin-top: auto;
    padding: 0 var(--space-2);
  }

  .collapsed .rail-footer {
    flex-direction: column;
    align-items: center;
  }

  .topbar {
    position: sticky;
    top: 0;
    z-index: var(--z-sticky);
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: calc(var(--control-lg) + var(--space-2));
    padding: var(--safe-top) calc(var(--space-2) + var(--safe-right)) 0
      calc(var(--space-2) + var(--safe-left));
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface-sunken);
  }

  .spacer {
    flex: 1;
  }

  .create-host {
    display: inline-flex;
  }

  .drawer-nav {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .drawer-home {
    align-self: flex-start;
    margin: 0 var(--space-2) var(--space-2);
    padding: var(--space-1) var(--space-2);
  }

  .content {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .notices {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) calc(var(--space-4) + var(--safe-right)) 0 var(--space-4);
  }

  main {
    flex: 1;
    min-width: 0;
    padding-inline-end: var(--safe-right);
    padding-bottom: var(--safe-bottom);
  }

  .narrow main {
    padding-inline-start: var(--safe-left);
  }

  main:focus {
    outline: none;
  }
</style>
