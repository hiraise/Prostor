import { describe, expect, it } from 'vitest';

import type { TimeContext } from './instant.ts';
import {
  dateInputFormat,
  formatCalendarDateForInput,
  formatInstantForInput,
  parseCalendarDate,
  parseDateInput,
  parseInstant,
} from './parse.ts';

const ru: TimeContext = { locale: 'ru', timeZone: 'Europe/Berlin' };
const en: TimeContext = { locale: 'en', timeZone: 'America/New_York' };
const NOW = Date.UTC(2026, 9, 21, 12, 0);

describe('dateInputFormat', () => {
  it('derives the order, separator and clock from the language', () => {
    expect(dateInputFormat('ru')).toEqual({
      order: ['day', 'month', 'year'],
      separator: '.',
      hour12: false,
    });
    expect(dateInputFormat('en')).toEqual({
      order: ['month', 'day', 'year'],
      separator: '/',
      hour12: true,
    });
  });
});

describe('parseDateInput', () => {
  const date = (input: string, context: TimeContext): unknown => {
    const result = parseDateInput(input, context, { now: NOW });
    return result.ok ? result.value : result.error;
  };

  it('UI-030: reads dates in the order of the language', () => {
    expect(date('29.03.2026', ru)).toEqual({ date: { year: 2026, month: 3, day: 29 }, time: null });
    expect(date('3/29/2026', en)).toEqual({ date: { year: 2026, month: 3, day: 29 }, time: null });
    expect(date(' 1.4.2026 ', ru)).toEqual({ date: { year: 2026, month: 4, day: 1 }, time: null });
  });

  it('always accepts ISO dates and date-times', () => {
    const iso = { date: { year: 2026, month: 3, day: 29 }, time: null };
    expect(date('2026-03-29', ru)).toEqual(iso);
    expect(date('2026-03-29', en)).toEqual(iso);
    expect(date('2026-03-29T15:15', en)).toEqual({ ...iso, time: { hour: 15, minute: 15 } });
  });

  it('reads times with a 24-hour or 12-hour clock', () => {
    expect(date('29.03.2026 15:15', ru)).toMatchObject({ time: { hour: 15, minute: 15 } });
    expect(date('29.03.2026, 7:05', ru)).toMatchObject({ time: { hour: 7, minute: 5 } });
    expect(date('3/29/2026 3:15 PM', en)).toMatchObject({ time: { hour: 15, minute: 15 } });
    expect(date('3/29/2026 12:05 am', en)).toMatchObject({ time: { hour: 0, minute: 5 } });
    expect(date('3/29/2026 12:30 p.m.', en)).toMatchObject({ time: { hour: 12, minute: 30 } });
  });

  it('completes a missing year and expands 2-digit years around now', () => {
    expect(date('29.03', ru)).toMatchObject({ date: { year: 2026, month: 3, day: 29 } });
    expect(date('29.03.26', ru)).toMatchObject({ date: { year: 2026 } });
    expect(date('29.03.76', ru)).toMatchObject({ date: { year: 2076 } });
    expect(date('29.03.77', ru)).toMatchObject({ date: { year: 1977 } });
    expect(date('29.03.99', ru)).toMatchObject({ date: { year: 1999 } });
  });

  it('reports what is wrong instead of throwing', () => {
    expect(date('   ', ru)).toBe('empty');
    expect(date('завтра', ru)).toBe('format');
    expect(date('29.03.202', ru)).toBe('format');
    expect(date('2026-03', ru)).toBe('format');
    expect(date('31.04.2026', ru)).toBe('invalid_date');
    expect(date('29.02.2026', ru)).toBe('invalid_date');
    expect(date('29.02.2028', ru)).toMatchObject({ date: { year: 2028, month: 2, day: 29 } });
    expect(date('29/03/2026', en)).toBe('invalid_date');
    expect(date('29.03.2026 25:00', ru)).toBe('invalid_time');
    expect(date('3/29/2026 13:00 PM', en)).toBe('invalid_time');
  });
});

describe('parseCalendarDate', () => {
  it('rejects a time', () => {
    expect(parseCalendarDate('29.03.2026', ru)).toEqual({
      ok: true,
      value: { year: 2026, month: 3, day: 29 },
    });
    expect(parseCalendarDate('29.03.2026 10:00', ru)).toEqual({
      ok: false,
      error: 'unexpected_time',
    });
  });
});

describe('parseInstant', () => {
  const instant = (input: string, context: TimeContext): string | null => {
    const result = parseInstant(input, context, { now: NOW });
    return result.ok ? new Date(result.value).toISOString() : null;
  };

  it('UI-030: reads wall time in the viewer zone', () => {
    expect(instant('21.10.2026 14:00', ru)).toBe('2026-10-21T12:00:00.000Z');
    expect(instant('21.10.2026', ru)).toBe('2026-10-20T22:00:00.000Z');
    const noon = parseInstant('21.10.2026', ru, { defaultTime: { hour: 12, minute: 0 } });
    expect(noon).toEqual({ ok: true, value: Date.UTC(2026, 9, 21, 10, 0) });
  });

  it('moves a time skipped by DST forward and takes the first of a repeated one', () => {
    // Berlin: 02:00 → 03:00 on 29 March; 03:00 → 02:00 on 25 October.
    expect(instant('29.03.2026 02:30', ru)).toBe('2026-03-29T01:30:00.000Z');
    expect(instant('25.10.2026 02:30', ru)).toBe('2026-10-25T00:30:00.000Z');
    // New York: 02:00 → 03:00 on 8 March; 02:00 → 01:00 on 1 November.
    expect(instant('3/8/2026 2:30 AM', en)).toBe('2026-03-08T07:30:00.000Z');
    expect(instant('11/1/2026 1:30 AM', en)).toBe('2026-11-01T05:30:00.000Z');
  });

  it('passes parse errors through', () => {
    expect(parseInstant('31.04.2026', ru)).toEqual({ ok: false, error: 'invalid_date' });
  });
});

describe('the instance date format (ADM-016)', () => {
  // A pattern replaces the language's order and separator for typed dates; ISO always works.
  const dmyDot: TimeContext = { ...en, dateFormat: 'dmy_dot' };
  const dmyName: TimeContext = { ...ru, dateFormat: 'dmy_name' };
  const iso: TimeContext = { ...ru, dateFormat: 'iso' };
  const date = (input: string, context: TimeContext): unknown => {
    const result = parseDateInput(input, context, { now: NOW });
    return result.ok ? result.value : result.error;
  };

  it('reads typed dates in the pattern order, whatever the language', () => {
    expect(date('31.12.2000 23:59', dmyDot)).toEqual({
      date: { year: 2000, month: 12, day: 31 },
      time: { hour: 23, minute: 59 },
    });
    // The same English speaker's "3/29/2026" no longer reads: the pattern's order wins.
    expect(date('3.29.2026', dmyDot)).toBe('invalid_date');
    // Name months are typed as numbers.
    expect(date('31.12.2000', dmyName)).toMatchObject({ date: { year: 2000, month: 12 } });
    // Two numbers of the ISO pattern are month and day, its without-year derivative.
    expect(date('12-31', iso)).toMatchObject({ date: { year: 2026, month: 12, day: 31 } });
    expect(date('2000-12-31T23:59', iso)).toMatchObject({
      date: { year: 2000, month: 12, day: 31 },
    });
  });

  it('writes the editing form in the pattern, on the 24-hour clock', () => {
    expect(formatCalendarDateForInput({ year: 2026, month: 3, day: 9 }, dmyDot)).toBe('09.03.2026');
    expect(formatCalendarDateForInput({ year: 2026, month: 3, day: 9 }, iso)).toBe('2026-03-09');
    const value = Date.UTC(2026, 2, 29, 13, 15); // 09:15 in New York, 15:15 in Berlin
    expect(formatInstantForInput(value, dmyDot)).toBe('29.03.2026 09:15');
    expect(formatInstantForInput(value, iso)).toBe('2026-03-29 15:15');
    for (const context of [dmyDot, dmyName, iso]) {
      const typed = formatInstantForInput(value, context);
      expect(parseInstant(typed, context)).toEqual({ ok: true, value });
    }
  });
});

describe('formatting for input', () => {
  it('writes what the parser reads back', () => {
    expect(formatCalendarDateForInput({ year: 2026, month: 3, day: 9 }, ru)).toBe('09.03.2026');
    expect(formatCalendarDateForInput({ year: 2026, month: 3, day: 9 }, en)).toBe('03/09/2026');
    for (const context of [ru, en]) {
      for (const value of [Date.UTC(2026, 2, 29, 13, 15), Date.UTC(2026, 9, 25, 3, 5)]) {
        const typed = formatInstantForInput(value, context);
        expect(parseInstant(typed, context)).toEqual({ ok: true, value });
      }
    }
    expect(formatInstantForInput(Date.UTC(2026, 2, 29, 13, 15), en)).toBe('03/29/2026 9:15 AM');
    expect(formatInstantForInput(Date.UTC(2026, 2, 29, 13, 15), ru)).toBe('29.03.2026 15:15');
  });
});
