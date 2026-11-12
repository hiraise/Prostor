// Whom an issue is limited to (ISS-041), in words.
import type { Audience } from '#lib/api/schema.gen.ts';
import { groupName, personName } from '#lib/features/access/index.ts';
import { getLocale } from '#lib/i18n/locale.svelte.ts';

import * as m from '../messages.ts';

/** Limited to somebody: the issue is not visible to everyone who sees its project. */
export function isRestricted(audience: Audience): boolean {
  return audience.users.length > 0 || audience.groups.length > 0;
}

/** «Видят Разработчики, Анна и автор»; `null` when there is no restriction. */
export function audienceText(audience: Audience): string | null {
  if (!isRestricted(audience)) return null;
  const names = [...audience.groups.map(groupName), ...audience.users.map(personName)];
  const list = new Intl.ListFormat(getLocale(), { type: 'conjunction', style: 'narrow' });
  // The list joins with commas only: the message adds the reporter with its own «и».
  return m.restrictedTo({ names: list.format(names).replace(/ (and|и) /, ', ') });
}
