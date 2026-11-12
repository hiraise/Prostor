// Values of an issue's fields as the page shows them and as editors read and send them
// (FLD-001…FLD-014, ISS-001, ISS-019).
import type {
  FieldKind,
  FieldValueInput,
  GroupRef,
  HeldValue,
  IssueField,
  UserRef,
  WorkSchedule,
} from '#lib/api/schema.gen.ts';
import { parseIsoDate, toIsoDate } from '#lib/time/calendar.ts';
import { formatDuration, parseDuration } from '#lib/time/duration.ts';
import type { TimeContext } from '#lib/time/instant.ts';
import {
  formatCalendarDateForInput,
  formatInstantForInput,
  parseCalendarDate,
  parseInstant,
} from '#lib/time/parse.ts';

/** The field holds no value. */
export function isEmpty(entry: IssueField): boolean {
  switch (entry.field.kind) {
    case 'string':
    case 'text':
      return !entry.text;
    case 'integer':
      return entry.integer === null;
    case 'float':
      return entry.float === null;
    case 'date':
      return entry.date === null;
    case 'date_time':
      return entry.date_time === null;
    case 'duration':
      return entry.minutes === null;
    case 'user':
      return entry.users.length === 0;
    case 'group':
      return entry.groups.length === 0;
    default:
      return entry.values.length === 0;
  }
}

/** Text fields are blocks under the description; every other field is a row of the panel. */
export function isBlock(entry: IssueField): boolean {
  return entry.field.kind === 'text';
}

/** A required field the issue still lacks (FLD-057). */
export function isMissing(entry: IssueField): boolean {
  return entry.required && isEmpty(entry);
}

/** Kinds edited by typing; the others are chosen from options. */
export type TypedKind = 'string' | 'integer' | 'float' | 'date' | 'date_time' | 'duration';

export function isTyped(kind: FieldKind): kind is TypedKind {
  return (
    kind === 'string' ||
    kind === 'integer' ||
    kind === 'float' ||
    kind === 'date' ||
    kind === 'date_time' ||
    kind === 'duration'
  );
}

/** Why typed text is not a value of the field. */
export type TypedProblem = 'number' | 'fraction' | 'date' | 'date_time' | 'duration' | 'range';

/** Typed text read for a field: what to send and what the row shows meanwhile. */
export type Typed =
  | { ok: true; value: FieldValueInput | null; shown: Partial<IssueField> }
  | { ok: false; problem: TypedProblem };

export interface TypedContext {
  time: TimeContext;
  schedule: WorkSchedule;
}

const NUMBER = /^[+-]?\d+(?:[.,]\d+)?$/u;

/** Reads a number typed with `,` or `.` as the decimal mark and spaces between thousands. */
export function parseNumber(
  text: string,
  integer: boolean,
): { ok: true; value: number } | { ok: false; problem: 'number' | 'fraction' | 'range' } {
  const compact = text.replace(/[\s\u00a0\u202f]/gu, '');
  if (!NUMBER.test(compact)) return { ok: false, problem: 'number' };
  const value = Number(compact.replace(',', '.'));
  if (integer && !Number.isInteger(value)) return { ok: false, problem: 'fraction' };
  if (integer ? !Number.isSafeInteger(value) : !Number.isFinite(value)) {
    return { ok: false, problem: 'range' };
  }
  return { ok: true, value };
}

/** Reads typed text for a field; empty text clears it. */
export function readTyped(kind: TypedKind, text: string, context: TypedContext): Typed {
  const trimmed = text.trim();
  if (trimmed === '') {
    const cleared: Partial<IssueField> = { text: null, integer: null, float: null };
    return {
      ok: true,
      value: null,
      shown: { ...cleared, date: null, date_time: null, minutes: null },
    };
  }
  switch (kind) {
    case 'string':
      return { ok: true, value: text, shown: { text } };
    case 'integer':
    case 'float': {
      const read = parseNumber(trimmed, kind === 'integer');
      if (!read.ok) return read;
      return {
        ok: true,
        value: read.value,
        shown: kind === 'integer' ? { integer: read.value } : { float: read.value },
      };
    }
    case 'date': {
      const read = parseCalendarDate(trimmed, context.time);
      if (!read.ok) return { ok: false, problem: 'date' };
      const iso = toIsoDate(read.value);
      return { ok: true, value: iso, shown: { date: iso } };
    }
    case 'date_time': {
      const read = parseInstant(trimmed, context.time);
      if (!read.ok) return { ok: false, problem: 'date_time' };
      const iso = new Date(read.value).toISOString();
      return { ok: true, value: iso, shown: { date_time: iso } };
    }
    case 'duration': {
      const read = parseDuration(trimmed, context.schedule);
      if (!read.ok)
        return { ok: false, problem: read.problem === 'invalid' ? 'duration' : 'range' };
      return { ok: true, value: read.minutes, shown: { minutes: read.minutes } };
    }
  }
}

/** The value of a typed field as its input shows it for editing. */
export function typedText(entry: IssueField, context: TypedContext): string {
  const { locale } = context.time;
  switch (entry.field.kind) {
    case 'string':
      return entry.text ?? '';
    case 'integer':
      return entry.integer === null ? '' : String(entry.integer);
    case 'float':
      if (entry.float === null) return '';
      return locale === 'ru' ? String(entry.float).replace('.', ',') : String(entry.float);
    case 'date': {
      const date = entry.date === null ? null : parseIsoDate(entry.date);
      return date ? formatCalendarDateForInput(date, context.time) : '';
    }
    case 'date_time':
      return entry.date_time === null
        ? ''
        : formatInstantForInput(Date.parse(entry.date_time), context.time);
    case 'duration':
      return entry.minutes === null ? '' : formatDuration(entry.minutes, locale, context.schedule);
    default:
      return '';
  }
}

/** Something a field of the chosen kinds can hold. */
export type Choice =
  | { kind: 'value'; value: HeldValue }
  | { kind: 'user'; user: UserRef }
  | { kind: 'group'; group: GroupRef };

/** The ID a choice is sent by. */
export function choiceId(choice: Choice): number {
  return choice.kind === 'value'
    ? choice.value.id
    : choice.kind === 'user'
      ? choice.user.id
      : choice.group.id;
}

/** The choices the field holds now. */
export function heldChoices(entry: IssueField): Choice[] {
  if (entry.field.kind === 'user') return entry.users.map((user) => ({ kind: 'user', user }));
  if (entry.field.kind === 'group') return entry.groups.map((group) => ({ kind: 'group', group }));
  return entry.values.map((value) => ({ kind: 'value', value }));
}

/** The field holding exactly `choices`. */
export function withChoices(entry: IssueField, choices: readonly Choice[]): IssueField {
  const values: HeldValue[] = [];
  const users: UserRef[] = [];
  const groups: GroupRef[] = [];
  for (const choice of choices) {
    if (choice.kind === 'value') {
      const { id, name, color, description, archived, resolved } = choice.value;
      values.push({ id, name, color, description, archived, resolved });
    } else if (choice.kind === 'user') users.push(choice.user);
    else groups.push(choice.group);
  }
  return { ...entry, values, users, groups };
}

/** What `PUT …/fields/{field}` sends for a field holding `choices`: one ID, IDs, or nothing. */
export function toInput(entry: IssueField, choices: readonly Choice[]): FieldValueInput | null {
  if (choices.length === 0) return null;
  const ids = choices.map(choiceId);
  return entry.field.multi ? ids : (ids[0] ?? null);
}
