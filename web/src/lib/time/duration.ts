// Durations (TIME-005, TIME-006): stored as whole minutes, typed and shown in weeks, days, hours
// and minutes of the work schedule. The same rules as the server's parser
// (crates/domain/src/duration.rs); both run the cases in crates/domain/src/duration_cases.json.
// Input: numbers with a unit letter in the order w, d, h, m, each at most once, spaces allowed;
// Latin (w d h m) or Russian (н д ч м) letters in any case.
import type { Locale } from '#lib/i18n/locale.svelte.ts';

/** Longest duration in minutes; the server refuses more. */
export const DURATION_MAX = 1_000_000_000;

/** The work day and week durations are counted in (from the bootstrap). */
export interface WorkSchedule {
  day_minutes: number;
  week_days: number;
}

/** Until the schedule settings exist (TIME-001…TIME-003): 8-hour days, 5-day weeks. */
export const DEFAULT_SCHEDULE: WorkSchedule = { day_minutes: 480, week_days: 5 };

/** Why typed text is not a duration; the codes are the server's. */
export type DurationProblem = 'required' | 'invalid' | 'out_of_range';

export type ParsedDuration =
  { ok: true; minutes: number } | { ok: false; problem: DurationProblem };

type Unit = 0 | 1 | 2 | 3; // minute, hour, day, week

function unitOf(letter: string): Unit | null {
  switch (letter.toLowerCase()) {
    case 'w':
    case 'н':
      return 3;
    case 'd':
    case 'д':
      return 2;
    case 'h':
    case 'ч':
      return 1;
    case 'm':
    case 'м':
      return 0;
    default:
      return null;
  }
}

function minutesOf(unit: Unit, schedule: WorkSchedule): number {
  switch (unit) {
    case 0:
      return 1;
    case 1:
      return 60;
    case 2:
      return schedule.day_minutes;
    case 3:
      return schedule.day_minutes * schedule.week_days;
  }
}

const TOKEN = /\s*(\d+)\s*(\p{L})(?!\p{L})/uy;

/** Reads a duration into minutes. */
export function parseDuration(
  text: string,
  schedule: WorkSchedule = DEFAULT_SCHEDULE,
): ParsedDuration {
  const input = text.trim();
  if (!input) return { ok: false, problem: 'required' };
  TOKEN.lastIndex = 0;
  let total = 0;
  let last: Unit | null = null;
  while (TOKEN.lastIndex < input.length) {
    const start = TOKEN.lastIndex;
    const match = TOKEN.exec(input);
    if (match?.index !== start) {
      // Digits too long to read count as too long, everything else as unreadable.
      return /^\s*\d{10,}/u.test(input.slice(start))
        ? { ok: false, problem: 'out_of_range' }
        : { ok: false, problem: 'invalid' };
    }
    const [, digits = '', letter = ''] = match;
    const unit = unitOf(letter);
    if (unit === null || (last !== null && unit >= last)) return { ok: false, problem: 'invalid' };
    last = unit;
    const number = Number(digits);
    if (number > DURATION_MAX) return { ok: false, problem: 'out_of_range' };
    total += number * minutesOf(unit, schedule);
    if (total > DURATION_MAX) return { ok: false, problem: 'out_of_range' };
  }
  return { ok: true, minutes: total };
}

const LETTERS: Readonly<Record<Locale, readonly [string, string, string, string]>> = {
  en: ['w', 'd', 'h', 'm'],
  ru: ['н', 'д', 'ч', 'м'],
};

/** Shows minutes in the largest units of the schedule: 90 → "1h 30m", 0 → "0m". */
export function formatDuration(
  minutes: number,
  locale: Locale,
  schedule: WorkSchedule = DEFAULT_SCHEDULE,
): string {
  const letters = LETTERS[locale];
  let rest = Math.max(0, Math.trunc(minutes));
  const sizes = [schedule.day_minutes * schedule.week_days, schedule.day_minutes, 60, 1];
  const parts: string[] = [];
  sizes.forEach((size, index) => {
    if (size <= 0) return;
    const count = Math.floor(rest / size);
    rest %= size;
    if (count > 0) parts.push(`${String(count)}${letters[index] ?? ''}`);
  });
  return parts.length > 0 ? parts.join(' ') : `0${letters[3]}`;
}
