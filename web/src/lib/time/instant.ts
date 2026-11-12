// Instants: points in time. The API sends them as RFC 3339 strings in UTC.
import type { DateFormat } from '#lib/api/schema.gen.ts';
import type { Locale } from '#lib/i18n/locale.svelte.ts';

/** Who sees the time: their interface language and time zone. */
export interface TimeContext {
  readonly locale: Locale;
  /** IANA zone name, such as `Europe/Berlin`. */
  readonly timeZone: string;
  /**
   * The instance's explicit date pattern (ADM-016); absent (`'locale'` is never stored) when
   * dates follow the language. Replaces the compact styles, not `long` tooltips.
   */
  readonly dateFormat?: Exclude<DateFormat, 'locale'>;
}

/** An instant: an RFC 3339 string, a Date or milliseconds since the Unix epoch. */
export type InstantInput = string | Date | number;

// Strict RFC 3339: any number of fraction digits (the server may send nanoseconds), which
// Date.parse does not accept in every browser.
const RFC_3339 =
  /^(\d{4})-(\d{2})-(\d{2})[Tt ](\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?(?:([Zz])|([+-])(\d{2}):(\d{2}))$/;

function parseRfc3339(value: string): number | null {
  const match = RFC_3339.exec(value);
  if (!match) return null;
  const [
    ,
    year,
    month,
    day,
    hour,
    minute,
    second,
    fraction = '',
    zulu,
    sign,
    offsetHour,
    offsetMinute,
  ] = match;
  const date = new Date(0);
  date.setUTCFullYear(Number(year), Number(month) - 1, Number(day));
  date.setUTCHours(
    Number(hour),
    Number(minute),
    Number(second),
    Number(fraction.padEnd(3, '0').slice(0, 3)),
  );
  // Reject values Date silently rolls over (30 February, 25:00).
  if (date.getUTCDate() !== Number(day) || date.getUTCHours() !== Number(hour)) return null;
  const offset = zulu
    ? 0
    : (sign === '-' ? -1 : 1) * (Number(offsetHour) * 60 + Number(offsetMinute));
  return date.getTime() - offset * 60_000;
}

/** Milliseconds since the epoch, or null when the value is not a valid instant. */
export function toInstant(value: InstantInput): number | null {
  const instant =
    typeof value === 'string'
      ? parseRfc3339(value)
      : typeof value === 'number'
        ? value
        : value.getTime();
  return instant !== null && Number.isFinite(instant) ? instant : null;
}

/** Like `toInstant`, for values that must be valid; throws RangeError otherwise. */
export function requireInstant(value: InstantInput): number {
  const instant = toInstant(value);
  if (instant === null) throw new RangeError(`Not an instant: ${String(value)}`);
  return instant;
}
