// Time zones: wall time of an instant in a zone and back. Intl carries the zone database with its
// history of offsets and DST rules, so no tables ship with the app.
import { calendarDateToUtc, type CalendarDate } from './calendar.ts';
import { dateTimeFormat } from './intl.ts';

/** A time of day on a wall clock. */
export interface WallTime {
  /** 0–23. */
  readonly hour: number;
  /** 0–59. */
  readonly minute: number;
}

/** Date and time of day as a clock in some zone shows them. */
export interface ZonedDateTime {
  readonly date: CalendarDate;
  readonly time: WallTime;
}

const MINUTE_MS = 60_000;
const DAY_MS = 86_400_000;

export function isValidTimeZone(timeZone: string): boolean {
  try {
    new Intl.DateTimeFormat('en-US', { timeZone });
    return true;
  } catch {
    return false;
  }
}

/** The zone of this browser; UTC when the browser does not say. */
export function browserTimeZone(): string {
  try {
    const zone = new Intl.DateTimeFormat().resolvedOptions().timeZone;
    return zone && isValidTimeZone(zone) ? zone : 'UTC';
  } catch {
    return 'UTC';
  }
}

interface WallClock {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
}

function wallClock(instant: number, timeZone: string): WallClock {
  const format = dateTimeFormat('en-US', {
    timeZone,
    hourCycle: 'h23',
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
    hour: 'numeric',
    minute: 'numeric',
    second: 'numeric',
  });
  const clock: WallClock = { year: 0, month: 0, day: 0, hour: 0, minute: 0, second: 0 };
  for (const part of format.formatToParts(instant)) {
    if (part.type in clock) clock[part.type as keyof WallClock] = Number(part.value);
  }
  return clock;
}

/** Offset of the zone from UTC at an instant, in milliseconds (Berlin in summer: +2 h). */
export function zoneOffset(instant: number, timeZone: string): number {
  const clock = wallClock(instant, timeZone);
  const wall =
    calendarDateToUtc(clock) +
    clock.hour * 3_600_000 +
    clock.minute * MINUTE_MS +
    clock.second * 1000;
  const wholeSeconds = instant - (((instant % 1000) + 1000) % 1000);
  return wall - wholeSeconds;
}

/** The date and time a clock in the zone shows at the instant. */
export function zonedDateTime(instant: number, timeZone: string): ZonedDateTime {
  const clock = wallClock(instant, timeZone);
  return {
    date: { year: clock.year, month: clock.month, day: clock.day },
    time: { hour: clock.hour, minute: clock.minute },
  };
}

/** The calendar date in the zone at the instant. */
export function calendarDateInZone(instant: number, timeZone: string): CalendarDate {
  return zonedDateTime(instant, timeZone).date;
}

/**
 * The instant when a clock in the zone shows this date and time. When the clock jumps forward
 * (a DST gap) the time does not exist and moves forward by the gap; when it repeats (an overlap)
 * the earlier of the two instants wins.
 */
export function wallTimeToInstant(date: CalendarDate, time: WallTime, timeZone: string): number {
  const wall = calendarDateToUtc(date) + time.hour * 3_600_000 + time.minute * MINUTE_MS;
  // A transition near the wall time separates two offsets: the one a day before and a day after.
  const before = zoneOffset(wall - DAY_MS, timeZone);
  const after = zoneOffset(wall + DAY_MS, timeZone);
  const matches = [wall - before, wall - after].filter(
    (instant) => instant + zoneOffset(instant, timeZone) === wall,
  );
  if (matches.length > 0) return Math.min(...matches);
  // The wall time falls into a gap: read it with the offset in force before the jump.
  return wall - before;
}

/** Every time zone this browser knows, by IANA name, with UTC and the browser's own included. */
export function timeZoneNames(): string[] {
  const names = new Set<string>(['UTC', browserTimeZone()]);
  try {
    for (const name of Intl.supportedValuesOf('timeZone')) names.add(name);
  } catch {
    // An older browser: the two above still let the person choose.
  }
  return [...names].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

/** An offset from UTC as people read it: `UTC+03:00`, `UTC−05:30`, `UTC±00:00`. */
export function formatOffset(offsetMs: number): string {
  const minutes = Math.round(Math.abs(offsetMs) / MINUTE_MS);
  const sign = offsetMs > 0 ? '+' : offsetMs < 0 ? '−' : '±';
  const hours = String(Math.floor(minutes / 60)).padStart(2, '0');
  return `UTC${sign}${hours}:${String(minutes % 60).padStart(2, '0')}`;
}
