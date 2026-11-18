<script lang="ts">
  // The account menu (UI-003): who is signed in, the account page, theme and language, and signing
  // out. The button shows the person's picture or initials, and their name where the rail has room.
  import Keyboard from '@lucide/svelte/icons/keyboard';
  import LogOut from '@lucide/svelte/icons/log-out';
  import UserRound from '@lucide/svelte/icons/user-round';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import type { CurrentUser } from '#lib/api/schema.gen.ts';
  import { getLocale, setLocale, type Locale } from '#lib/i18n/locale.svelte.ts';
  import { signOut } from '#lib/session/index.ts';
  import {
    getThemePreference,
    setThemePreference,
    type ThemePreference,
  } from '#lib/theme/theme.svelte.ts';
  import Avatar from '#lib/ui/Avatar.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import type { Placement } from '#lib/ui/position.ts';
  import { showError } from '#lib/ui/toast.svelte.ts';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import * as m from './messages.ts';

  interface Props {
    user: CurrentUser;
    /** Show the name next to the initials (the expanded rail). */
    withName?: boolean;
    placement?: Placement;
    /** Opens the shortcut reference (UI-020); unset hides the help entry. */
    onhelp?: () => void;
  }

  let { user, withName = false, placement, onhelp }: Props = $props();

  const themes: [ThemePreference, () => string][] = [
    ['system', m.themeSystem],
    ['light', m.themeLight],
    ['dark', m.themeDark],
  ];
  const languages: [Locale, string][] = [
    ['en', 'English'],
    ['ru', 'Русский'],
  ];

  async function leave(): Promise<void> {
    try {
      // The app shell hears that the session ended and opens the sign-in page.
      await signOut();
    } catch (error) {
      showError(error);
    }
  }

  const items: MenuEntry[] = $derived([
    {
      type: 'action',
      label: m.account(),
      icon: UserRound,
      onSelect: () => void goto(resolve('/(app)/account')),
    },
    ...(onhelp
      ? [
          {
            type: 'action' as const,
            label: m.shortcutsTitle(),
            icon: Keyboard,
            onSelect: onhelp,
          },
        ]
      : []),
    { type: 'separator' },
    {
      type: 'group',
      label: m.theme(),
      items: themes.map(([value, label]) => ({
        type: 'radio',
        label: label(),
        checked: getThemePreference() === value,
        onSelect: () => {
          setThemePreference(value);
        },
      })),
    },
    {
      type: 'group',
      label: m.language(),
      items: languages.map(([value, label]) => ({
        type: 'radio',
        label,
        lang: value,
        checked: getLocale() === value,
        onSelect: () => {
          setLocale(value);
        },
      })),
    },
    { type: 'separator' },
    { type: 'action', label: m.signOut(), icon: LogOut, onSelect: () => void leave() },
  ]);
  const label = $derived(m.accountMenu({ name: user.full_name }));
</script>

<Menu {label} {items} {placement}>
  {#snippet trigger(props)}
    <!-- With the name shown, the name is the button's text, so what people see is what a
         screen reader says; with initials only, the label names the button. -->
    <button
      type="button"
      class={['account', { 'with-name': withName }]}
      aria-label={withName ? undefined : label}
      {@attach withName ? undefined : tooltip(label)}
      {...props as HTMLButtonAttributes}
    >
      <Avatar name={user.full_name} size="sm" color={user.avatar_color} src={user.avatar_url} />
      {#if withName}
        <span class="name"><span class="sr-only">{`${m.account()}: `}</span>{user.full_name}</span>
      {/if}
    </button>
  {/snippet}
</Menu>

<style>
  .account {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-width: var(--control-md);
    min-height: var(--control-md);
    padding: 0 var(--space-1);
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    cursor: pointer;
  }

  .with-name {
    flex: 1;
    justify-content: flex-start;
    min-width: 0;
    padding: 0 var(--space-2);
  }

  .account:hover,
  .account[aria-expanded='true'] {
    background: var(--color-hover);
  }

  .name {
    overflow: hidden;
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
