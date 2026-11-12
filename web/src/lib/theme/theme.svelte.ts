// Light and dark themes (docs/design.md). The preference follows the operating system unless the
// user picks one; until the personal setting (UI-014) is stored on the server it lives in this
// browser and is shared by its tabs.
import { MediaQuery } from 'svelte/reactivity';

import { onStoredChange, readStored, writeStored } from '#lib/storage.ts';

export const THEME_PREFERENCES = ['system', 'light', 'dark'] as const;
export type ThemePreference = (typeof THEME_PREFERENCES)[number];
export type Theme = 'light' | 'dark';

const STORAGE_KEY = 'trackflow.theme';

let preference = $state<ThemePreference>('system');
const systemDark = new MediaQuery('(prefers-color-scheme: dark)', false);

export function isThemePreference(value: unknown): value is ThemePreference {
  return typeof value === 'string' && (THEME_PREFERENCES as readonly string[]).includes(value);
}

export function getThemePreference(): ThemePreference {
  return preference;
}

/** The theme in effect: the user's choice, or the system's when the choice is "system". */
export function getTheme(): Theme {
  if (preference === 'system') return systemDark.current ? 'dark' : 'light';
  return preference;
}

export function setThemePreference(value: ThemePreference): void {
  preference = value;
  writeStored(STORAGE_KEY, value === 'system' ? null : value);
}

/** Puts the theme on <html>, where the tokens read it (tokens.css). */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = theme;
}

/**
 * Reads the stored preference, applies the theme before the app renders and keeps it in sync with
 * the system and other tabs. Call once at startup; returns a function that stops syncing.
 */
export function initTheme(): () => void {
  const stored = readStored(STORAGE_KEY);
  preference = isThemePreference(stored) ? stored : 'system';
  applyTheme(getTheme());

  const stopEffect = $effect.root(() => {
    $effect(() => {
      applyTheme(getTheme());
    });
  });
  const stopStorage = onStoredChange(STORAGE_KEY, (value) => {
    preference = isThemePreference(value) ? value : 'system';
  });
  return () => {
    stopEffect();
    stopStorage();
  };
}
