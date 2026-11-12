// TIME-005, TIME-006: the browser reads and shows durations exactly as the server does: both run
// the shared cases of crates/domain/src/duration_cases.json.
import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { formatDuration, parseDuration, type WorkSchedule } from './duration.ts';

interface Cases {
  parse: { input: string; minutes?: number; problem?: string; day?: number }[];
  format: { minutes: number; en: string; ru: string; day?: number }[];
}

const cases = JSON.parse(
  readFileSync(
    new URL('../../../../crates/domain/src/duration_cases.json', import.meta.url),
    'utf8',
  ),
) as Cases;

function schedule(day?: number): WorkSchedule {
  return { day_minutes: day ?? 480, week_days: 5 };
}

describe('durations', () => {
  it.each(cases.parse)('TIME-006: parses $input', ({ input, minutes, problem, day }) => {
    const parsed = parseDuration(input, schedule(day));
    if (minutes !== undefined) expect(parsed).toEqual({ ok: true, minutes });
    else expect(parsed).toEqual({ ok: false, problem });
  });

  it.each(cases.format)('TIME-005: shows $minutes minutes', ({ minutes, en, ru, day }) => {
    expect(formatDuration(minutes, 'en', schedule(day))).toBe(en);
    expect(formatDuration(minutes, 'ru', schedule(day))).toBe(ru);
  });
});
