// The dates of a sprint for people (BRD-083): «29 окт. – 11 нояб.», «С 29 окт.», «Без дат».
import type { Sprint } from '#lib/api/schema.gen.ts';
import { parseIsoDate, type CalendarDate } from '#lib/time/calendar.ts';
import { getTimeContext } from '#lib/time/context.svelte.ts';
import { formatCalendarDate, formatCalendarRange } from '#lib/time/format.ts';
import { calendarDateInZone } from '#lib/time/zone.ts';

import * as m from '../messages.ts';

export function sprintDates(sprint: Pick<Sprint, 'starts_on' | 'ends_on'>): string {
  const context = getTimeContext();
  const starts = sprint.starts_on === null ? null : parseIsoDate(sprint.starts_on);
  const ends = sprint.ends_on === null ? null : parseIsoDate(sprint.ends_on);
  if (starts && ends) return formatCalendarRange(starts, ends, context);
  if (starts) return m.fromDate({ date: formatCalendarDate(starts, context) });
  if (ends) return m.untilDate({ date: formatCalendarDate(ends, context) });
  return m.noDates();
}

/** Today in the person's time zone. */
export function today(now = Date.now()): CalendarDate {
  return calendarDateInZone(now, getTimeContext().timeZone);
}
