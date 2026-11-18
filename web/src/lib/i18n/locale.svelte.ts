// The interface language (D-018). Until the personal language setting (UI-013) is stored on the
// server, the choice is kept in this browser and otherwise follows the browser's languages.
import { readStored, writeStored, onStoredChange } from '#lib/storage.ts';

export const LOCALES = ['en', 'ru'] as const;
export type Locale = (typeof LOCALES)[number];
/** The language every text is written in first; also the fallback for unknown browser languages. */
export const BASE_LOCALE: Locale = 'en';

const STORAGE_KEY = 'prostor.locale';

let current = $state<Locale>(BASE_LOCALE);

export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (LOCALES as readonly string[]).includes(value);
}

/** The current interface language. Reading it inside a component or `$derived` tracks changes. */
export function getLocale(): Locale {
  return current;
}

/**
 * The language to start with: a stored choice, else the first browser language we have,
 * else the base language.
 */
export function detectLocale(stored: string | null, languages: readonly string[]): Locale {
  if (isLocale(stored)) return stored;
  for (const tag of languages) {
    const language = tag.toLowerCase().split('-')[0];
    if (isLocale(language)) return language;
  }
  return BASE_LOCALE;
}

function apply(locale: Locale): void {
  current = locale;
  if (typeof document !== 'undefined') document.documentElement.lang = locale;
}

/** Switches the language everywhere at once and remembers the choice in this browser. */
export function setLocale(locale: Locale): void {
  apply(locale);
  writeStored(STORAGE_KEY, locale);
}

/**
 * Picks the starting language and follows changes made in other tabs. Call once before the
 * app renders; returns a function that stops following.
 */
export function initLocale(): () => void {
  const languages = typeof navigator === 'undefined' ? [] : navigator.languages;
  apply(detectLocale(readStored(STORAGE_KEY), languages));
  return onStoredChange(STORAGE_KEY, (value) => {
    if (isLocale(value)) apply(value);
  });
}
