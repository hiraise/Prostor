// What the project forms say about a rejected name or key (PRJ-002, PRJ-003, PRJ-018): the
// server's field problems in the project's own words, for the creation page and the settings.
import type { FieldError } from '#lib/api/schema.gen.ts';
import { getLocale } from '#lib/i18n/locale.svelte.ts';
import { formatNumber } from '#lib/i18n/message.ts';
import { fieldErrorTexts, type OwnFieldTexts } from '#lib/i18n/messages/errors.ts';

import * as m from './messages.ts';

/** The largest number of a project's first issue (PRJ-004); the server's limit. */
export const FIRST_NUMBER_MAX = 1_000_000_000;

/** What a first issue number may be, in the viewer's language. */
export function firstNumberRange(): string {
  return m.firstNumberRange({ max: formatNumber(getLocale(), FIRST_NUMBER_MAX) });
}

/** The first issue number typed, or `null` when it is not a whole number in range. */
export function parseFirstNumber(text: string): number | null {
  const typed = text.trim();
  if (!/^\d{1,10}$/.test(typed)) return null;
  const number = Number(typed);
  return number >= 1 && number <= FIRST_NUMBER_MAX ? number : null;
}

const own: OwnFieldTexts = {
  name: { taken: m.nameTaken },
  key: {
    taken: m.keyTaken,
    invalid: m.keyInvalid,
    too_short: m.keyLength,
    too_long: m.keyLength,
  },
  first_number: { out_of_range: firstNumberRange, invalid: firstNumberRange },
};

/** Texts under each rejected field of a project form, by field name. */
export function projectFieldTexts(fields: readonly FieldError[]): Record<string, string> {
  return fieldErrorTexts(fields, own);
}
