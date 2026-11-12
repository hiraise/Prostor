// Calendar dates: a day without a time or a time zone (a due date, a sprint start). They are never
// shifted by a zone; the API sends them as `YYYY-MM-DD`.

export interface CalendarDate {
  readonly year: number;
  /** 1–12. */
  readonly month: number;
  /** 1–31. */
  readonly day: number;
}

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;
const DAY_MS = 86_400_000;
const MONTH_DAYS = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

function isLeapYear(year: number): boolean {
  return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
}

export function daysInMonth(year: number, month: number): number {
  if (month === 2 && isLeapYear(year)) return 29;
  return MONTH_DAYS[month - 1] ?? 0;
}

export function isValidCalendarDate(year: number, month: number, day: number): boolean {
  return (
    Number.isInteger(year) &&
    Number.isInteger(month) &&
    Number.isInteger(day) &&
    month >= 1 &&
    month <= 12 &&
    day >= 1 &&
    day <= daysInMonth(year, month)
  );
}

/** A validated calendar date, or null when the day does not exist (31 April). */
export function calendarDate(year: number, month: number, day: number): CalendarDate | null {
  return isValidCalendarDate(year, month, day) ? { year, month, day } : null;
}

/** Whether a string is a calendar date in API form (`YYYY-MM-DD`) rather than an instant. */
export function isCalendarDateString(value: string): boolean {
  return ISO_DATE.test(value);
}

/** Reads a `YYYY-MM-DD` date; null when the form is wrong or the day does not exist. */
export function parseIsoDate(value: string): CalendarDate | null {
  const match = ISO_DATE.exec(value);
  if (!match) return null;
  return calendarDate(Number(match[1]), Number(match[2]), Number(match[3]));
}

/** The API form of a calendar date: `YYYY-MM-DD`. */
export function toIsoDate(date: CalendarDate): string {
  const pad = (value: number, length: number): string => String(value).padStart(length, '0');
  return `${pad(date.year, 4)}-${pad(date.month, 2)}-${pad(date.day, 2)}`;
}

/** Midnight UTC of the date: formatting it with the UTC zone shows the same day everywhere. */
export function calendarDateToUtc(date: CalendarDate): number {
  const value = new Date(0);
  // setUTCFullYear keeps years below 100 as they are (Date.UTC would add 1900).
  value.setUTCFullYear(date.year, date.month - 1, date.day);
  return value.getTime();
}

/** Whole days from `from` to `to`: positive when `to` is later. */
export function daysBetween(from: CalendarDate, to: CalendarDate): number {
  return Math.round((calendarDateToUtc(to) - calendarDateToUtc(from)) / DAY_MS);
}
