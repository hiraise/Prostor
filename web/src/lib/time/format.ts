// Rendering instants and calendar dates for people (UI-030). Every function takes the viewer's
// language, time zone and the instance's date format explicitly; `context.svelte.ts` supplies the
// current ones. An explicit instance pattern (ADM-016) replaces the compact styles; `long` stays
// spelled out by the language.
import {
  calendarDateToUtc,
  daysBetween,
  isCalendarDateString,
  isValidCalendarDate,
  parseIsoDate,
  toIsoDate,
  type CalendarDate,
} from './calendar.ts';
import { requireInstant, toInstant, type InstantInput, type TimeContext } from './instant.ts';
import { dateTimeFormat, relativeTimeFormat } from './intl.ts';
import { calendarDateInZone } from './zone.ts';

/**
 * How a date is spelled:
 * - `short` (default): "Mar 29, 2026" / "29 мар. 2026 г.";
 * - `numeric`: "03/29/2026" / "29.03.2026" (tables, dense lists);
 * - `long`: "Sunday, March 29, 2026" / "воскресенье, 29 марта 2026 г." (tooltips).
 */
export type DateStyle = 'short' | 'numeric' | 'long';

/** `auto` (default) shows the year only when it is not the current one in the viewer's zone. */
export type YearMode = 'auto' | 'always' | 'never';

/** What part of an instant to show. */
export type InstantKind = 'datetime' | 'date' | 'time';

export interface DateOptions {
  readonly style?: DateStyle;
  readonly year?: YearMode;
  /** The current time, for `year: 'auto'` and relative phrases; defaults to the clock. */
  readonly now?: number;
}

export interface InstantOptions extends DateOptions {
  readonly kind?: InstantKind;
}

const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;
const DAY_MS = 86_400_000;
/** Relative phrases name days up to a week away; further dates are shown as dates. */
const RELATIVE_DAYS = 7;

const TIME_FIELDS = { hour: 'numeric', minute: '2-digit' } as const;
/** The instance patterns (ADM-016) write no 12-hour clock, so their times are always 24-hour. */
const PATTERN_TIME_FIELDS = { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' } as const;

function dateFields(style: DateStyle, withYear: boolean): Intl.DateTimeFormatOptions {
  const year = withYear ? ({ year: 'numeric' } as const) : {};
  switch (style) {
    case 'numeric':
      return { day: '2-digit', month: '2-digit', ...year };
    case 'short':
      return { day: 'numeric', month: 'short', ...year };
    case 'long':
      return { weekday: 'long', day: 'numeric', month: 'long', ...year };
  }
}

/** How one instance pattern spells a date: which parts, in what order, joined how (ADM-016). */
interface Pattern {
  /** Order of the date numbers; the year drops out of the without-year derivative. */
  readonly order: readonly ('day' | 'month' | 'year')[];
  /** A number (`31.12`) or the language's abbreviated name (`31 дек.`). */
  readonly month: '2-digit' | 'short';
  /** Zero-padded day and month numbers; name patterns show plain numbers. */
  readonly padded: boolean;
  /** Between the date numbers. */
  readonly separator: string;
  /** Between the date and the time: `, ` or a space. */
  readonly timeGlue: string;
}

const PATTERNS = {
  dmy_dot: {
    order: ['day', 'month', 'year'],
    month: '2-digit',
    padded: true,
    separator: '.',
    timeGlue: ', ',
  },
  dmy_name: {
    order: ['day', 'month', 'year'],
    month: 'short',
    padded: false,
    separator: ' ',
    timeGlue: ', ',
  },
  iso: {
    order: ['year', 'month', 'day'],
    month: '2-digit',
    padded: true,
    separator: '-',
    timeGlue: ' ',
  },
} as const satisfies Record<string, Pattern>;

/** A formatter both modes share: `Intl.DateTimeFormat` already fits the language-driven one. */
interface DateRenderer {
  format(value: number | Date): string;
  formatRange(start: number | Date, end: number | Date): string;
}

const intlRenderer = (format: Intl.DateTimeFormat): DateRenderer => format;

/**
 * One instance pattern as a renderer: `Intl` formats the parts (the month name in the viewer's
 * language), the pattern joins them its own way, so the separators and the order are exactly the
 * administrator's choice and not the language's.
 */
function patternRenderer(
  context: TimeContext,
  pattern: Pattern,
  kind: InstantKind,
  withYear: boolean,
  zone: string,
): DateRenderer {
  const fields: Intl.DateTimeFormatOptions = {};
  if (kind !== 'time') {
    for (const part of pattern.order) {
      if (part === 'year') {
        if (withYear) fields.year = 'numeric';
      } else if (part === 'month') {
        fields.month = pattern.month;
      } else {
        fields.day = pattern.padded ? '2-digit' : 'numeric';
      }
    }
  }
  if (kind !== 'date') Object.assign(fields, PATTERN_TIME_FIELDS);
  const format = dateTimeFormat(context.locale, { timeZone: zone, ...fields });

  const render = (value: number): string => {
    const parts = format.formatToParts(value);
    const byType = new Map(
      parts.filter((part) => part.type !== 'literal').map((part) => [part.type, part.value]),
    );
    const time = `${byType.get('hour') ?? ''}:${byType.get('minute') ?? ''}`;
    if (kind === 'time') return time;
    const date = pattern.order
      .filter((part) => part !== 'year' || withYear)
      .map((part) => byType.get(part) ?? '')
      .join(pattern.separator);
    return kind === 'date' ? date : date + pattern.timeGlue + time;
  };
  return {
    format: (value) => render(Number(value)),
    formatRange: (start, end) => `${render(Number(start))} – ${render(Number(end))}`,
  };
}

function showYear(mode: YearMode, years: readonly number[], currentYear: () => number): boolean {
  if (mode !== 'auto') return mode === 'always';
  const current = currentYear();
  return years.some((year) => year !== current);
}

/**
 * The renderer for one value: the instance pattern when the context carries one (compact styles
 * only — `long` stays spelled out by the language), else the language-driven `Intl` format.
 */
function dateRenderer(
  context: TimeContext,
  kind: InstantKind,
  style: DateStyle,
  withYear: boolean,
  zone: string,
): DateRenderer {
  const { dateFormat } = context;
  if (dateFormat !== undefined && style !== 'long') {
    return patternRenderer(context, PATTERNS[dateFormat], kind, withYear, zone);
  }
  const fields =
    kind === 'time'
      ? TIME_FIELDS
      : kind === 'date'
        ? dateFields(style, withYear)
        : { ...dateFields(style, withYear), ...TIME_FIELDS };
  return intlRenderer(dateTimeFormat(context.locale, { timeZone: zone, ...fields }));
}

function instantFormat(
  context: TimeContext,
  kind: InstantKind,
  style: DateStyle,
  withYear: boolean,
): DateRenderer {
  return dateRenderer(context, kind, style, withYear, context.timeZone);
}

function calendarFormat(context: TimeContext, style: DateStyle, withYear: boolean): DateRenderer {
  // Calendar dates are kept at midnight UTC and read in UTC, so no zone can move them.
  return dateRenderer(context, 'date', style, withYear, 'UTC');
}

/** An instant in the viewer's zone: "Mar 29, 3:15 PM" / "29 мар., 15:15". Throws on an invalid value. */
export function formatInstant(
  value: InstantInput,
  context: TimeContext,
  options: InstantOptions = {},
): string {
  const instant = requireInstant(value);
  const { kind = 'datetime', style = 'short', year = 'auto', now = Date.now() } = options;
  const withYear =
    kind !== 'time' &&
    showYear(
      year,
      [calendarDateInZone(instant, context.timeZone).year],
      () => calendarDateInZone(now, context.timeZone).year,
    );
  return instantFormat(context, kind, style, withYear).format(instant);
}

/** A calendar date, the same day in every zone: "Mar 29" / "29 мар.". */
export function formatCalendarDate(
  date: CalendarDate,
  context: TimeContext,
  options: DateOptions = {},
): string {
  const { style = 'short', year = 'auto', now = Date.now() } = options;
  const withYear = showYear(
    year,
    [date.year],
    () => calendarDateInZone(now, context.timeZone).year,
  );
  return calendarFormat(context, style, withYear).format(calendarDateToUtc(date));
}

/** A span between two instants: "Mar 29, 3:15 – 5:15 PM". Throws on invalid values. */
export function formatInstantRange(
  start: InstantInput,
  end: InstantInput,
  context: TimeContext,
  options: InstantOptions = {},
): string {
  const from = requireInstant(start);
  const to = requireInstant(end);
  const { kind = 'datetime', style = 'short', year = 'auto', now = Date.now() } = options;
  const withYear =
    kind !== 'time' &&
    showYear(
      year,
      [
        calendarDateInZone(from, context.timeZone).year,
        calendarDateInZone(to, context.timeZone).year,
      ],
      () => calendarDateInZone(now, context.timeZone).year,
    );
  return instantFormat(context, kind, style, withYear).formatRange(from, to);
}

/** A span of calendar dates: "Mar 29 – Apr 2" / "29 мар. – 2 апр.". */
export function formatCalendarRange(
  start: CalendarDate,
  end: CalendarDate,
  context: TimeContext,
  options: DateOptions = {},
): string {
  const { style = 'short', year = 'auto', now = Date.now() } = options;
  const withYear = showYear(
    year,
    [start.year, end.year],
    () => calendarDateInZone(now, context.timeZone).year,
  );
  return calendarFormat(context, style, withYear).formatRange(
    calendarDateToUtc(start),
    calendarDateToUtc(end),
  );
}

/**
 * A relative phrase for an instant: "now", "5 minutes ago", "in 3 hours", "yesterday",
 * "in 2 days"; a week or more away it is the date itself. Throws on an invalid value.
 */
export function formatRelative(
  value: InstantInput,
  context: TimeContext,
  options: { readonly now?: number } = {},
): string {
  const instant = requireInstant(value);
  const now = options.now ?? Date.now();
  const diff = instant - now;
  const distance = Math.abs(diff);
  const phrase = relativeTimeFormat(context.locale);
  if (distance < MINUTE_MS) return phrase.format(0, 'second');
  if (distance < HOUR_MS) return phrase.format(Math.trunc(diff / MINUTE_MS), 'minute');
  // Days count calendar days in the viewer's zone, so 01:00 today and 23:00 the day before
  // are "2 hours" apart, while anything a day or more away is "yesterday", "2 days ago"…
  const days = daysBetween(
    calendarDateInZone(now, context.timeZone),
    calendarDateInZone(instant, context.timeZone),
  );
  if (distance < DAY_MS || days === 0) return phrase.format(Math.trunc(diff / HOUR_MS), 'hour');
  if (Math.abs(days) < RELATIVE_DAYS) return phrase.format(days, 'day');
  return formatInstant(instant, context, { kind: 'date', now });
}

/** A relative phrase for a calendar date: "today", "tomorrow", "3 days ago"; else the date. */
export function formatRelativeDate(
  date: CalendarDate,
  context: TimeContext,
  options: { readonly now?: number } = {},
): string {
  const now = options.now ?? Date.now();
  const days = daysBetween(calendarDateInZone(now, context.timeZone), date);
  if (Math.abs(days) < RELATIVE_DAYS) return relativeTimeFormat(context.locale).format(days, 'day');
  return formatCalendarDate(date, context, { now });
}

/** A value `<Time>` shows: an instant, or a calendar date (a `YYYY-MM-DD` string or object). */
export type TimeValue = InstantInput | CalendarDate;

/** What `<Time>` shows: `kind: 'relative'` picks a relative phrase. */
export type TimeKind = InstantKind | 'relative';

export interface TimeView {
  /** Visible text. */
  readonly text: string;
  /** Machine-readable value for the `datetime` attribute. */
  readonly datetime: string;
  /** Full absolute text for the tooltip; absent when it would repeat `text`. */
  readonly title: string | undefined;
}

type Classified = { readonly date: CalendarDate } | { readonly instant: number } | null;

function classify(value: TimeValue): Classified {
  if (typeof value === 'string' && isCalendarDateString(value)) {
    const date = parseIsoDate(value);
    return date && { date };
  }
  if (typeof value === 'string' || typeof value === 'number' || value instanceof Date) {
    const instant = toInstant(value);
    return instant === null ? null : { instant };
  }
  return isValidCalendarDate(value.year, value.month, value.day) ? { date: value } : null;
}

/**
 * Everything `<Time>` renders for a value; null when the value is not a valid instant or date.
 * Calendar dates ignore the time part of `kind`.
 */
export function describeTime(
  value: TimeValue,
  context: TimeContext,
  options: {
    readonly kind?: TimeKind;
    readonly style?: DateStyle;
    readonly year?: YearMode;
    readonly now?: number;
  } = {},
): TimeView | null {
  const { kind = 'datetime', style = 'short', year = 'auto', now = Date.now() } = options;
  const view = (text: string, datetime: string, title: string): TimeView => ({
    text,
    datetime,
    title: title === text ? undefined : title,
  });

  const target = classify(value);
  if (target === null) return null;
  if ('date' in target) {
    const { date } = target;
    const text =
      kind === 'relative'
        ? formatRelativeDate(date, context, { now })
        : formatCalendarDate(date, context, { style, year, now });
    const title = formatCalendarDate(date, context, { style: 'long', year: 'always' });
    return view(text, toIsoDate(date), title);
  }

  const { instant } = target;
  const text =
    kind === 'relative'
      ? formatRelative(instant, context, { now })
      : formatInstant(instant, context, { kind, style, year, now });
  const title = formatInstant(instant, context, {
    kind: 'datetime',
    style: 'long',
    year: 'always',
  });
  return view(text, new Date(instant).toISOString(), title);
}
