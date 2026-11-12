<script lang="ts">
  // Theme and language switches (until the account menu, UI-003, and the profile settings,
  // UI-013/UI-014, take them over). Language names are shown in their own language.
  import Settings2 from '@lucide/svelte/icons/settings-2';

  import { getLocale, setLocale, type Locale } from '#lib/i18n/locale.svelte.ts';
  import {
    getThemePreference,
    setThemePreference,
    type ThemePreference,
  } from '#lib/theme/theme.svelte.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import type { Placement } from '#lib/ui/position.ts';

  import * as m from './messages.ts';

  let { placement }: { placement?: Placement } = $props();

  const themes: [ThemePreference, () => string][] = [
    ['system', m.themeSystem],
    ['light', m.themeLight],
    ['dark', m.themeDark],
  ];
  const languages: [Locale, string][] = [
    ['en', 'English'],
    ['ru', 'Русский'],
  ];

  const items: MenuEntry[] = $derived([
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
  ]);
</script>

<Menu label={m.preferences()} {items} {placement}>
  {#snippet trigger(props)}
    <IconButton icon={Settings2} label={m.preferences()} {...props} />
  {/snippet}
</Menu>
