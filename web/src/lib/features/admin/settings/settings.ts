// The general settings of the instance (ADM-002; plan step 1.11 §4): the calls of the «Настройки»
// screen of the administration area. The settings come as one list of values; the mail settings
// are not among them — they have their own screen and API.
import { api } from '#lib/api/client.ts';
import type { AdminSettings, AdminSettingsChange } from '#lib/api/schema.gen.ts';

/** The settings as they stand; every value with its default and whether a row is stored. */
export function getAdminSettings(signal?: AbortSignal): Promise<AdminSettings> {
  return api.get('/admin/settings', { signal });
}

/**
 * Saves a change set: each present key changes, `null` returns the key to its default, absent
 * keys stay. The answer is how the settings stand now. The schema's change type carries no keys —
 * any JSON object is a valid body — so the screen passes its map straight through.
 */
export function saveAdminSettings(change: Record<string, unknown>): Promise<AdminSettings> {
  return api.put('/admin/settings', { body: change as AdminSettingsChange });
}

/** The value of a setting as it stands, or `fallback` when the key is missing. */
export function settingValue<T>(settings: AdminSettings, key: string, fallback: T): T {
  const found = settings.settings.find((setting) => setting.key === key);
  return (found?.value as T | undefined) ?? fallback;
}
