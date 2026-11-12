// Reading dates and times people type (UI-030). The numeric order and separator come from the
// viewer's language (ru: 29.03.2026, en: 03/29/2026), or from the instance's date pattern when
// the administrator chose one (ADM-016); ISO `2026-03-29` always works.
import { calendarDate, type CalendarDate } from './calendar.ts';
import type { TimeContext } from './instant.ts';
import { dateTimeFormat } from './intl.ts';
import { calendarDateInZone, wallTimeToInstant, zonedDateTime, type WallTime } from './zone.ts';

/** Why typed text is not a date. */
export type DateParseError =
  /** Nothing typed. */
  | 'empty'
  /** Not shaped like a date (and time). */
  | 'format'
  /** Shaped right, but the day does not exist (31.04). */
  | 'invalid_date'
  /** Shaped right, but the time does not exist (25:00). */
  | 'invalid_time'
  /** A time where only a date is expected. */
  | 'unexpected_time';

export type ParseResult<T> =
  { readonly ok: true; readonly value: T } | { readonly ok: false; readonly error: DateParseError };

/** A typed date with an optional time of day. */
export interface DateInput {
  readonly date: CalendarDate;
  readonly time: WallTime | null;
}

export type DatePart = 'day' | 'month' | 'year';

/** How a language writes numeric dates and times. */
export interface DateInputFormat {
  /** Order of the numbers: ru `day, month, year`, en `month, day, year`. */
  readonly order: readonly DatePart[];
  /** Between the numbers: ru `.`, en `/`. */
  readonly separator: string;
  /** Whether times use a 12-hour clock with AM/PM. */
  readonly hour12: boolean;
}

export interface ParseOptions {
  /** The current time: completes a missing year and decides the century of a 2-digit year. */
  readonly now?: number;
}

const formats = new Map<string, DateInputFormat>();

/** The numeric date order, separator and clock of a language. */
export function dateInputFormat(locale: string): DateInputFormat {
  let format = formats.get(locale);
  if (format) return format;
  const parts = dateTimeFormat(locale, {
    timeZone: 'UTC',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).formatToParts(Date.UTC(2026, 10, 22));
  const order = parts
    .map((part) => part.type)
    .filter((type): type is DatePart => type === 'day' || type === 'month' || type === 'year');
  const separator = parts.find((part) => part.type === 'literal')?.value.trim() ?? '';
  const hourCycle = dateTimeFormat(locale, { hour: 'numeric' }).resolvedOptions().hourCycle;
  format = {
    order: order.length === 3 ? order : ['day', 'month', 'year'],
    separator: separator === '' ? '/' : separator,
    hour12: hourCycle === 'h11' || hourCycle === 'h12',
  };
  formats.set(locale, format);
  return format;
}

/**
 * How a context's dates are typed: the instance pattern's order and separator when the
 * administrator chose one (all patterns write a 24-hour clock), else the language's. Name months
 * are typed as numbers (`31.12.2000`), the way the pattern's editing form shows them.
 */
const PATTERN_INPUTS: Record<NonNullable<TimeContext['dateFormat']>, DateInputFormat> = {
  dmy_dot: { order: ['day', 'month', 'year'], separator: '.', hour12: false },
  dmy_name: { order: ['day', 'month', 'year'], separator: '.', hour12: false },
  iso: { order: ['year', 'month', 'day'], separator: '-', hour12: false },
};

function inputFormat(context: TimeContext): DateInputFormat {
  return context.dateFormat === undefined
    ? dateInputFormat(context.locale)
    : PATTERN_INPUTS[context.dateFormat];
}

// Date: two or three numbers separated by `.`, `/` or `-`; then optionally a time `H:mm` after a
// space, a comma or `T`, with an optional AM/PM marker.
const INPUT =
  /^(\d{1,4})[./-](\d{1,2})(?:[./-](\d{1,4}))?\.?(?:(?:\s*,\s*|\s+|[Tt])(\d{1,2}):(\d{2})(?:\s*([AaPp])\.?(?:[Mm]\.?)?)?)?$/;

/** The year a 2-digit year means: the one within 50 years of now. */
function expandYear(digits: string, currentYear: number): number {
  const value = Number(digits);
  if (digits.length === 4) return value;
  let year = currentYear - (currentYear % 100) + value;
  if (year > currentYear + 50) year -= 100;
  else if (year <= currentYear - 50) year += 100;
  return year;
}

function readTime(
  hourText: string,
  minuteText: string,
  marker: string | undefined,
): WallTime | null {
  let hour = Number(hourText);
  const minute = Number(minuteText);
  if (minute > 59) return null;
  if (marker === undefined) return hour <= 23 ? { hour, minute } : null;
  if (hour < 1 || hour > 12) return null;
  const pm = marker.toLowerCase() === 'p';
  if (hour === 12) hour = pm ? 12 : 0;
  else if (pm) hour += 12;
  return { hour, minute };
}

/** Reads a date with an optional time: "29.03.2026", "3/29/26 3:15 PM", "2026-03-29T15:15", "29.03". */
export function parseDateInput(
  input: string,
  context: TimeContext,
  options: ParseOptions = {},
): ParseResult<DateInput> {
  const text = input.trim();
  if (text === '') return { ok: false, error: 'empty' };
  const match = INPUT.exec(text);
  if (!match) return { ok: false, error: 'format' };
  const [, first = '', second = '', third, hourText, minuteText, marker] = match;

  let numbers: Partial<Record<DatePart, string>>;
  if (first.length === 4) {
    // Year first: ISO and its look-alikes.
    if (third === undefined) return { ok: false, error: 'format' };
    numbers = { year: first, month: second, day: third };
  } else {
    const order = inputFormat(context).order;
    const values = third === undefined ? [first, second] : [first, second, third];
    const parts = third === undefined ? order.filter((part) => part !== 'year') : order;
    numbers = Object.fromEntries(parts.map((part, index) => [part, values[index]]));
  }
  const { day = '', month = '', year } = numbers;
  if (day.length > 2 || month.length > 2) return { ok: false, error: 'format' };
  if (year !== undefined && year.length !== 2 && year.length !== 4) {
    return { ok: false, error: 'format' };
  }

  const currentYear = calendarDateInZone(options.now ?? Date.now(), context.timeZone).year;
  const date = calendarDate(
    year === undefined ? currentYear : expandYear(year, currentYear),
    Number(month),
    Number(day),
  );
  if (!date) return { ok: false, error: 'invalid_date' };
  if (hourText === undefined || minuteText === undefined) {
    return { ok: true, value: { date, time: null } };
  }
  const time = readTime(hourText, minuteText, marker);
  if (!time) return { ok: false, error: 'invalid_time' };
  return { ok: true, value: { date, time } };
}

/** Reads a calendar date (no time allowed). */
export function parseCalendarDate(
  input: string,
  context: TimeContext,
  options: ParseOptions = {},
): ParseResult<CalendarDate> {
  const result = parseDateInput(input, context, options);
  if (!result.ok) return result;
  if (result.value.time !== null) return { ok: false, error: 'unexpected_time' };
  return { ok: true, value: result.value.date };
}

/**
 * Reads a date and time as wall time in the viewer's zone and returns the instant (milliseconds
 * since the epoch). Without a time, `defaultTime` (midnight) is used. A time skipped by a DST
 * jump moves forward; a repeated one means its first occurrence.
 */
export function parseInstant(
  input: string,
  context: TimeContext,
  options: ParseOptions & { readonly defaultTime?: WallTime } = {},
): ParseResult<number> {
  const result = parseDateInput(input, context, options);
  if (!result.ok) return result;
  const time = result.value.time ?? options.defaultTime ?? { hour: 0, minute: 0 };
  return { ok: true, value: wallTimeToInstant(result.value.date, time, context.timeZone) };
}

const pad2 = (value: number): string => String(value).padStart(2, '0');

/** A calendar date as the viewer types it, for editing: "29.03.2026", "03/29/2026". */
export function formatCalendarDateForInput(date: CalendarDate, context: TimeContext): string {
  const { order, separator } = inputFormat(context);
  const values: Record<DatePart, string> = {
    day: pad2(date.day),
    month: pad2(date.month),
    year: String(date.year).padStart(4, '0'),
  };
  return order.map((part) => values[part]).join(separator);
}

/** An instant as the viewer types it, for editing: "29.03.2026 15:15", "03/29/2026 3:15 PM". */
export function formatInstantForInput(instant: number, context: TimeContext): string {
  const { date, time } = zonedDateTime(instant, context.timeZone);
  const day = formatCalendarDateForInput(date, context);
  if (!inputFormat(context).hour12) return `${day} ${pad2(time.hour)}:${pad2(time.minute)}`;
  const hour = time.hour % 12 === 0 ? 12 : time.hour % 12;
  return `${day} ${String(hour)}:${pad2(time.minute)} ${time.hour < 12 ? 'AM' : 'PM'}`;
}
