import { describe, expect, it } from 'vitest';

import {
  describeTime,
  formatCalendarDate,
  formatCalendarRange,
  formatInstant,
  formatInstantRange,
  formatRelative,
  formatRelativeDate,
} from './format.ts';
import type { TimeContext } from './instant.ts';

// Intl puts narrow and thin no-break spaces around AM/PM and range dashes.
const plain = (text: string | undefined): string | undefined =>
  text?.replace(/[\u00a0\u202f\u2009]/g, ' ');

const berlin = (locale: 'en' | 'ru'): TimeContext => ({ locale, timeZone: 'Europe/Berlin' });
const NOW = Date.UTC(2026, 9, 21, 12, 0); // 21 Oct 2026, 14:00 in Berlin

describe('formatInstant', () => {
  it('UI-030: shows instants in the viewer zone across DST changes', () => {
    const time = (iso: string, context: TimeContext): string | undefined =>
      plain(formatInstant(iso, context, { kind: 'time' }));
    // Berlin jumps from 02:00 to 03:00 at 01:00 UTC on 29 March 2026…
    expect(time('2026-03-29T00:30:00Z', berlin('en'))).toBe('1:30 AM');
    expect(time('2026-03-29T01:30:00Z', berlin('en'))).toBe('3:30 AM');
    // …and repeats 02:00–03:00 on 25 October 2026.
    expect(time('2026-10-25T00:30:00Z', berlin('ru'))).toBe('2:30');
    expect(time('2026-10-25T01:30:00Z', berlin('ru'))).toBe('2:30');
    const newYork: TimeContext = { locale: 'en', timeZone: 'America/New_York' };
    expect(time('2026-03-08T06:30:00Z', newYork)).toBe('1:30 AM');
    expect(time('2026-03-08T07:30:00Z', newYork)).toBe('3:30 AM');
  });

  it('drops the current year in auto mode and keeps it otherwise', () => {
    const at = '2026-03-29T13:15:00Z';
    expect(plain(formatInstant(at, berlin('en'), { now: NOW }))).toBe('Mar 29, 3:15 PM');
    expect(plain(formatInstant(at, berlin('ru'), { now: NOW }))).toBe('29 мар., 15:15');
    expect(plain(formatInstant(at, berlin('en'), { now: NOW, year: 'always' }))).toBe(
      'Mar 29, 2026, 3:15 PM',
    );
    expect(plain(formatInstant('2025-03-29T13:15:00Z', berlin('ru'), { now: NOW }))).toBe(
      '29 мар. 2025 г., 14:15',
    );
    expect(
      plain(formatInstant('2025-03-29T13:15:00Z', berlin('en'), { now: NOW, year: 'never' })),
    ).toBe('Mar 29, 2:15 PM');
  });

  it('decides the year in the viewer zone', () => {
    // 23:30 UTC on 31 December 2025 is already 2026 in Berlin.
    const newYear = '2025-12-31T23:30:00Z';
    expect(formatInstant(newYear, berlin('en'), { kind: 'date', now: NOW })).toBe('Jan 1');
    const utc: TimeContext = { locale: 'en', timeZone: 'UTC' };
    expect(formatInstant(newYear, utc, { kind: 'date', now: NOW })).toBe('Dec 31, 2025');
  });

  it('spells dates in numeric and long styles', () => {
    const at = '2026-03-29T13:15:00Z';
    expect(
      formatInstant(at, berlin('ru'), { kind: 'date', style: 'numeric', year: 'always' }),
    ).toBe('29.03.2026');
    expect(formatInstant(at, berlin('en'), { kind: 'date', style: 'numeric', now: NOW })).toBe(
      '03/29',
    );
    expect(
      plain(formatInstant(at, berlin('ru'), { kind: 'date', style: 'long', year: 'always' })),
    ).toBe('воскресенье, 29 марта 2026 г.');
  });

  it('rejects an invalid instant', () => {
    expect(() => formatInstant('yesterday', berlin('en'))).toThrow(RangeError);
  });
});

describe('formatCalendarDate', () => {
  it('UI-030: shows the same day in every zone', () => {
    const due = { year: 2026, month: 3, day: 29 };
    const kiritimati: TimeContext = { locale: 'en', timeZone: 'Pacific/Kiritimati' }; // UTC+14
    const pagoPago: TimeContext = { locale: 'en', timeZone: 'Pacific/Pago_Pago' }; // UTC−11
    expect(formatCalendarDate(due, kiritimati, { year: 'always' })).toBe('Mar 29, 2026');
    expect(formatCalendarDate(due, pagoPago, { year: 'always' })).toBe('Mar 29, 2026');
    // An instant, unlike a calendar date, falls on different days there.
    const instant = '2026-03-29T05:00:00Z';
    expect(formatInstant(instant, kiritimati, { kind: 'date', now: NOW })).toBe('Mar 29');
    expect(formatInstant(instant, pagoPago, { kind: 'date', now: NOW })).toBe('Mar 28');
  });

  it('follows the year mode', () => {
    expect(formatCalendarDate({ year: 2026, month: 10, day: 11 }, berlin('ru'), { now: NOW })).toBe(
      '11 окт.',
    );
    expect(
      formatCalendarDate({ year: 2027, month: 1, day: 5 }, berlin('en'), {
        now: NOW,
        style: 'numeric',
      }),
    ).toBe('01/05/2027');
  });
});

describe('ranges', () => {
  it('joins instants on one day', () => {
    const range = formatInstantRange('2026-03-29T13:15:00Z', '2026-03-29T15:15:00Z', berlin('en'), {
      now: NOW,
    });
    expect(plain(range)).toBe('Mar 29, 3:15 – 5:15 PM');
    const ru = formatInstantRange('2026-03-29T13:15:00Z', '2026-03-29T15:15:00Z', berlin('ru'), {
      now: NOW,
    });
    expect(plain(ru)).toBe('29 мар., 15:15–17:15');
  });

  it('joins calendar dates and shows the year when the range leaves the current one', () => {
    const start = { year: 2026, month: 3, day: 29 };
    const end = { year: 2026, month: 4, day: 2 };
    expect(plain(formatCalendarRange(start, end, berlin('en'), { now: NOW }))).toBe(
      'Mar 29 – Apr 2',
    );
    expect(plain(formatCalendarRange(start, end, berlin('ru'), { year: 'always' }))).toBe(
      '29 мар. – 2 апр. 2026 г.',
    );
    const newYear = formatCalendarRange(
      { year: 2025, month: 12, day: 30 },
      { year: 2026, month: 1, day: 2 },
      berlin('en'),
      { now: NOW },
    );
    expect(plain(newYear)).toBe('Dec 30, 2025 – Jan 2, 2026');
  });
});

describe('the instance date format (ADM-016)', () => {
  // The four patterns of the owner's decision, on the last minute of 2000 and a 2026 day.
  const dmyDot: TimeContext = { ...berlin('en'), dateFormat: 'dmy_dot' };
  const dmyName: TimeContext = { ...berlin('ru'), dateFormat: 'dmy_name' };
  const iso: TimeContext = { ...berlin('en'), dateFormat: 'iso' };
  const NEW_YEAR_2000 = '2000-12-31T22:59:00Z'; // 23:59 in Berlin

  it('spells each pattern with the year', () => {
    expect(formatInstant(NEW_YEAR_2000, dmyDot, { year: 'always' })).toBe('31.12.2000, 23:59');
    expect(formatInstant(NEW_YEAR_2000, dmyName, { year: 'always' })).toBe('31 дек. 2000, 23:59');
    expect(formatInstant(NEW_YEAR_2000, iso, { year: 'always' })).toBe('2000-12-31 23:59');
  });

  it('derives the date-only, without-year and day-month forms', () => {
    const at = '2026-03-29T13:15:00Z'; // 29 March 2026, 15:15 in Berlin (the current year)
    expect(formatInstant(at, dmyDot, { kind: 'date', year: 'always', now: NOW })).toBe(
      '29.03.2026',
    );
    expect(formatInstant(at, dmyDot, { now: NOW })).toBe('29.03, 15:15');
    expect(formatInstant(at, dmyDot, { kind: 'date', now: NOW })).toBe('29.03');
    expect(formatInstant(at, dmyName, { kind: 'date', now: NOW })).toBe('29 мар.');
    expect(formatInstant(at, iso, { kind: 'date', now: NOW })).toBe('03-29');
    expect(formatInstant(at, iso, { now: NOW })).toBe('03-29 15:15');
  });

  it('writes the time of a pattern on the 24-hour clock', () => {
    // The same language-driven value is "1:30 AM"; a pattern never writes AM/PM.
    expect(formatInstant('2026-03-29T00:30:00Z', dmyDot, { kind: 'time' })).toBe('01:30');
    expect(formatInstant('2026-03-29T13:15:00Z', iso, { kind: 'time' })).toBe('15:15');
  });

  it('names months in the viewer language, keeps the pattern order', () => {
    const english: TimeContext = { ...berlin('en'), dateFormat: 'dmy_name' };
    expect(formatInstant(NEW_YEAR_2000, english, { year: 'always' })).toBe('31 Dec 2000, 23:59');
  });

  it('keeps the long style spelled out by the language', () => {
    const view = describeTime(NEW_YEAR_2000, dmyDot, { now: NOW });
    expect(plain(view?.title)).toBe('Sunday, December 31, 2000 at 11:59 PM');
  });

  it('applies the pattern to calendar dates and ranges', () => {
    const start = { year: 2026, month: 3, day: 29 };
    const end = { year: 2026, month: 4, day: 2 };
    expect(formatCalendarDate(start, iso, { year: 'always', now: NOW })).toBe('2026-03-29');
    expect(plain(formatCalendarRange(start, end, dmyDot, { now: NOW }))).toBe('29.03 – 02.04');
    expect(
      plain(
        formatInstantRange('2026-03-29T13:15:00Z', '2026-03-29T15:15:00Z', dmyDot, {
          now: NOW,
        }),
      ),
    ).toBe('29.03, 15:15 – 29.03, 17:15');
  });

  it('reaches the surfaces through describeTime, and dates beyond the relative week', () => {
    const view = describeTime('2026-03-29', dmyDot, { kind: 'date', year: 'always', now: NOW });
    expect(view?.text).toBe('29.03.2026');
    // A phrase a week or more away falls back to the pattern's date.
    expect(formatRelative(Date.UTC(2026, 2, 29, 13, 15), dmyDot, { now: NOW })).toBe('29.03');
  });
});

describe('formatRelative', () => {
  const ago = (ms: number): number => NOW - ms;
  const MIN = 60_000;
  const HOUR = 3_600_000;
  const DAY = 86_400_000;
  const cases: [number, string, string][] = [
    [ago(30_000), 'now', 'сейчас'],
    [NOW + 30_000, 'now', 'сейчас'],
    [ago(5 * MIN + 59_000), '5 minutes ago', '5 минут назад'],
    [NOW + 3 * HOUR, 'in 3 hours', 'через 3 часа'],
    [ago(26 * HOUR), 'yesterday', 'вчера'],
    [ago(3 * DAY), '3 days ago', '3 дня назад'],
    [NOW + 2 * DAY, 'in 2 days', 'послезавтра'],
    [ago(10 * DAY), 'Oct 11', '11 окт.'],
  ];

  it.each(cases)('UI-030: phrases %d in both languages', (instant, en, ru) => {
    expect(formatRelative(instant, berlin('en'), { now: NOW })).toBe(en);
    expect(formatRelative(instant, berlin('ru'), { now: NOW })).toBe(ru);
  });

  it('counts hours, not days, within the last day around midnight', () => {
    // 00:30 on 22 October in Berlin; 23:00 the evening before.
    const now = Date.UTC(2026, 9, 21, 22, 30);
    expect(formatRelative(Date.UTC(2026, 9, 21, 21, 0), berlin('en'), { now })).toBe('1 hour ago');
  });

  it('phrases calendar dates by days in the viewer zone', () => {
    const relative = (day: number, locale: 'en' | 'ru'): string =>
      formatRelativeDate({ year: 2026, month: 10, day }, berlin(locale), { now: NOW });
    expect(relative(21, 'en')).toBe('today');
    expect(relative(22, 'ru')).toBe('завтра');
    expect(relative(24, 'en')).toBe('in 3 days');
    expect(relative(1, 'en')).toBe('Oct 1');
  });
});

describe('describeTime', () => {
  it('gives text, machine value and an absolute tooltip for an instant', () => {
    const view = describeTime('2026-10-21T11:54:30.123456789Z', berlin('en'), {
      kind: 'relative',
      now: NOW,
    });
    expect(view?.text).toBe('5 minutes ago');
    expect(view?.datetime).toBe('2026-10-21T11:54:30.123Z');
    expect(plain(view?.title)).toBe('Wednesday, October 21, 2026 at 1:54 PM');
  });

  it('treats YYYY-MM-DD as a calendar date', () => {
    const view = describeTime('2026-03-29', berlin('ru'), { kind: 'date', now: NOW });
    expect(view?.text).toBe('29 мар.');
    expect(view?.datetime).toBe('2026-03-29');
    expect(plain(view?.title)).toBe('воскресенье, 29 марта 2026 г.');
  });

  it('omits a tooltip that repeats the text', () => {
    const view = describeTime({ year: 2026, month: 3, day: 29 }, berlin('en'), {
      kind: 'date',
      style: 'long',
      year: 'always',
    });
    expect(view?.title).toBeUndefined();
  });

  it('returns nothing for values that are not dates', () => {
    expect(describeTime('2026-02-30', berlin('en'))).toBeNull();
    expect(describeTime('soon', berlin('en'))).toBeNull();
    expect(describeTime(Number.NaN, berlin('en'))).toBeNull();
    expect(describeTime({ year: 2026, month: 4, day: 31 }, berlin('en'))).toBeNull();
  });
});
