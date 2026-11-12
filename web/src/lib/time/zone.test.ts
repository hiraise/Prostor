import { describe, expect, it } from 'vitest';

import {
  calendarDateToUtc,
  daysBetween,
  parseIsoDate,
  toIsoDate,
  type CalendarDate,
} from './calendar.ts';
import { toInstant } from './instant.ts';
import {
  calendarDateInZone,
  isValidTimeZone,
  wallTimeToInstant,
  zoneOffset,
  zonedDateTime,
} from './zone.ts';

const HOUR = 3_600_000;

describe('zones', () => {
  it('knows offsets with their DST history', () => {
    expect(zoneOffset(Date.UTC(2026, 0, 15), 'Europe/Berlin')).toBe(HOUR);
    expect(zoneOffset(Date.UTC(2026, 6, 15), 'Europe/Berlin')).toBe(2 * HOUR);
    expect(zoneOffset(Date.UTC(2026, 6, 15), 'Asia/Kolkata')).toBe(5.5 * HOUR);
    expect(zoneOffset(Date.UTC(2026, 6, 15, 0, 0, 0, 999), 'America/New_York')).toBe(-4 * HOUR);
  });

  it('reads the wall clock of an instant', () => {
    expect(zonedDateTime(Date.UTC(2026, 2, 29, 22, 30), 'Europe/Moscow')).toEqual({
      date: { year: 2026, month: 3, day: 30 },
      time: { hour: 1, minute: 30 },
    });
    expect(calendarDateInZone(Date.UTC(2026, 2, 29, 5), 'Pacific/Pago_Pago')).toEqual({
      year: 2026,
      month: 3,
      day: 28,
    });
  });

  it('turns wall time into an instant', () => {
    const date: CalendarDate = { year: 2026, month: 7, day: 1 };
    expect(wallTimeToInstant(date, { hour: 9, minute: 0 }, 'Europe/Berlin')).toBe(
      Date.UTC(2026, 6, 1, 7, 0),
    );
    expect(wallTimeToInstant(date, { hour: 9, minute: 0 }, 'UTC')).toBe(Date.UTC(2026, 6, 1, 9, 0));
  });

  it('validates zone names', () => {
    expect(isValidTimeZone('Europe/Berlin')).toBe(true);
    expect(isValidTimeZone('Mars/Olympus')).toBe(false);
    expect(isValidTimeZone('')).toBe(false);
  });
});

describe('calendar dates', () => {
  it('read and write the API form', () => {
    expect(parseIsoDate('2028-02-29')).toEqual({ year: 2028, month: 2, day: 29 });
    expect(parseIsoDate('2026-02-29')).toBeNull();
    expect(parseIsoDate('2026-3-1')).toBeNull();
    expect(toIsoDate({ year: 33, month: 3, day: 1 })).toBe('0033-03-01');
  });

  it('count days without being affected by DST or old years', () => {
    expect(daysBetween({ year: 2026, month: 3, day: 28 }, { year: 2026, month: 3, day: 30 })).toBe(
      2,
    );
    expect(daysBetween({ year: 2028, month: 3, day: 1 }, { year: 2028, month: 2, day: 28 })).toBe(
      -2,
    );
    expect(new Date(calendarDateToUtc({ year: 33, month: 3, day: 1 })).getUTCFullYear()).toBe(33);
  });
});

describe('toInstant', () => {
  it('reads RFC 3339 with any fraction and offset', () => {
    expect(toInstant('2026-10-21T18:02:22.123456789Z')).toBe(Date.UTC(2026, 9, 21, 18, 2, 22, 123));
    expect(toInstant('2026-10-21t18:02:22z')).toBe(Date.UTC(2026, 9, 21, 18, 2, 22));
    expect(toInstant('2026-10-21T21:02:22+03:00')).toBe(Date.UTC(2026, 9, 21, 18, 2, 22));
    expect(toInstant('2026-10-21T13:32:22.5-04:30')).toBe(Date.UTC(2026, 9, 21, 18, 2, 22, 500));
  });

  it('accepts dates and numbers and rejects everything else', () => {
    expect(toInstant(new Date(5))).toBe(5);
    expect(toInstant(5)).toBe(5);
    expect(toInstant(new Date(Number.NaN))).toBeNull();
    expect(toInstant(Number.POSITIVE_INFINITY)).toBeNull();
    expect(toInstant('2026-02-30T00:00:00Z')).toBeNull();
    expect(toInstant('2026-10-21T24:00:00Z')).toBeNull();
    expect(toInstant('2026-10-21')).toBeNull();
    expect(toInstant('2026-10-21T18:02Z')).toBeNull();
  });
});
