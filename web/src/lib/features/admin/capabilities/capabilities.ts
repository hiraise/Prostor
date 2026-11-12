// The capabilities of the instance (ACC-064, ADM-023; plan step 1.11 §6.1): features switched on
// for chosen groups; the «Общие для всех» flags ride with the settings API.
import { api } from '#lib/api/client.ts';
import type {
  AdminSettings,
  AdminSettingsChange,
  Capability,
  CapabilityKey,
  Capabilities,
  Group,
  PageGroup,
} from '#lib/api/schema.gen.ts';
import { findGroups } from '#lib/features/access/index.ts';

/** Every capability with the groups it is on for. Needs `system.read`. */
export function listCapabilities(signal?: AbortSignal): Promise<Capabilities> {
  return api.get('/capabilities', { signal });
}

/** Sets the groups a capability is on for; an empty list switches it off. Needs `system.write`. */
export function setCapability(key: CapabilityKey, groups: number[]): Promise<Capability> {
  return api.put('/capabilities/{key}', { path: { key }, body: { groups } });
}

/** The instance-wide flags of the capabilities screen (ADM-023); they travel with the settings. */
export function instanceFlags(signal?: AbortSignal): Promise<AdminSettings> {
  return api.get('/admin/settings', { signal });
}

/**
 * Changes an instance-wide flag; `null` returns it to the default. The change type carries no
 * keys (any JSON object is a valid body), so the map passes straight through.
 */
export function saveInstanceFlag(key: string, value: boolean | null): Promise<AdminSettings> {
  return api.put('/admin/settings', { body: { [`flag.${key}`]: value } as AdminSettingsChange });
}

/** Every group and project team (an instance has few), as the multiselect lists them. */
export async function allGroups(): Promise<Group[]> {
  const groups: Group[] = [];
  let cursor: string | null = null;
  do {
    const page: PageGroup = await findGroups('', cursor, 'all');
    groups.push(...page.items);
    cursor = page.next_cursor;
  } while (cursor !== null);
  return groups;
}
