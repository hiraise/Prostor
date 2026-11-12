<script lang="ts">
  // The application frame (UI-001, UI-033): on wide screens a navigation rail that collapses to
  // icons; on phones a top bar whose menu button opens the navigation as a drawer (D-019). The
  // account menu (UI-003) sits at the bottom of the rail and at the end of the top bar. The
  // skip link and the single <main> landmark come first for keyboard and screen reader users.
  import Menu from '@lucide/svelte/icons/menu';
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close';
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open';
  import SquareKanban from '@lucide/svelte/icons/square-kanban';
  import type { Snippet } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';

  import {
    boardMessages,
    canCreateBoards,
    NewBoardHost,
    openNewBoard,
  } from '#lib/features/boards/index.ts';
  import { CreateMenu, NewIssueHost } from '#lib/features/issues/index.ts';
  import { NotificationBell } from '#lib/features/notifications/index.ts';
  import { NARROW } from '#lib/theme/breakpoints.ts';
  import Dialog from '#lib/ui/Dialog.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import type { MenuEntry } from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';

  import { currentUser } from '#lib/session/index.ts';

  import AccountMenu from './AccountMenu.svelte';
  import { missingFeatures } from './browser-support.ts';
  import Logo from './Logo.svelte';
  import * as m from './messages.ts';
  import { isRailCollapsed, setRailCollapsed } from './navigation.svelte.ts';
  import NavList from './NavList.svelte';
  import PreferencesMenu from './PreferencesMenu.svelte';
  import { homeHref } from './sections.ts';

  interface Props {
    pathname: string;
    children: Snippet;
  }

  let { pathname, children }: Props = $props();

  const narrow = new MediaQuery(NARROW, false);
  const user = $derived(currentUser());
  /** «Создать» entries of other features (UI-002): a board. */
  const createMore = $derived<MenuEntry[]>(
    canCreateBoards()
      ? [
          {
            type: 'action',
            label: boardMessages.createBoardItem(),
            icon: SquareKanban,
            onSelect: () => {
              openNewBoard();
            },
          },
        ]
      : [],
  );
  const collapsed = $derived(isRailCollapsed());
  let drawerOpen = $state(false);
  let oldBrowser = $state(missingFeatures().length > 0);

  // A drawer left open on a phone must not survive turning it into a wide screen.
  $effect(() => {
    if (!narrow.current) drawerOpen = false;
  });

  function closeDrawer(): void {
    drawerOpen = false;
  }
</script>

<!-- Links here lead home: homeHref() resolves the route (sections.ts). -->
<!-- eslint-disable svelte/no-navigation-without-resolve -->
<a class="skip" href="#main">{m.skipToContent()}</a>

<div class={['shell', { narrow: narrow.current, collapsed: collapsed && !narrow.current }]}>
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
      {#if user}<CreateMenu
          compact
          placement={{ side: 'bottom', align: 'end' }}
          more={createMore}
        />{/if}
      {#if user}
        <AccountMenu {user} placement={{ side: 'bottom', align: 'end' }} />
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
    <nav class="rail" aria-label={m.mainNavigation()}>
      <a class="home rail-home" href={homeHref()} aria-label={m.homeLink()}>
        <Logo compact={collapsed} />
      </a>
      {#if user}
        <div class="rail-create">
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

  .rail {
    position: sticky;
    top: 0;
    z-index: var(--z-rail);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100dvh;
    padding: calc(var(--space-3) + var(--safe-top)) 0 calc(var(--space-3) + var(--safe-bottom));
    padding-inline-start: var(--safe-left);
    border-inline-end: 1px solid var(--color-border);
    background: var(--color-surface-sunken);
    overflow: hidden auto;
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
