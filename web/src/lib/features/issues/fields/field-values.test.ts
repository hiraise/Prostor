// Reading typed field values and what the editors send (FLD-002, FLD-004…FLD-008, ISS-019).
import { describe, expect, it } from 'vitest';

import { DEFAULT_SCHEDULE } from '#lib/time/duration.ts';

import { ANNA, BORIS, field } from '../test-data.ts';
import {
  heldChoices,
  parseNumber,
  readTyped,
  toInput,
  typedText,
  withChoices,
  type TypedContext,
} from './field-values.ts';

const RU: TypedContext = {
  time: { locale: 'ru', timeZone: 'Europe/Moscow' },
  schedule: DEFAULT_SCHEDULE,
};
const EN: TypedContext = { ...RU, time: { locale: 'en', timeZone: 'UTC' } };

describe('FLD-004: numbers', () => {
  it('takes a comma or a point as the decimal mark and spaces between thousands', () => {
    expect(parseNumber('2,5', false)).toEqual({ ok: true, value: 2.5 });
    expect(parseNumber('2.5', false)).toEqual({ ok: true, value: 2.5 });
    expect(parseNumber('1 000', true)).toEqual({ ok: true, value: 1000 });
    expect(parseNumber('-3', true)).toEqual({ ok: true, value: -3 });
  });

  it('refuses fractions for integers and text that is not a number', () => {
    expect(parseNumber('2,5', true)).toEqual({ ok: false, problem: 'fraction' });
    expect(parseNumber('2,5,1', false)).toEqual({ ok: false, problem: 'number' });
    expect(parseNumber('abc', false)).toEqual({ ok: false, problem: 'number' });
    expect(parseNumber('99999999999999999999', true)).toEqual({ ok: false, problem: 'range' });
  });

  it('shows a float with the language’s decimal mark for editing', () => {
    const entry = field(1, 'Score', 'float', { float: 2.5 });
    expect(typedText(entry, RU)).toBe('2,5');
    expect(typedText(entry, EN)).toBe('2.5');
  });
});

describe('FLD-005, FLD-006: dates', () => {
  it('reads the language’s order and ISO, and sends ISO', () => {
    expect(readTyped('date', '29.03.2026', RU)).toEqual({
      ok: true,
      value: '2026-03-29',
      shown: { date: '2026-03-29' },
    });
    expect(readTyped('date', '2026-03-29', EN)).toMatchObject({ ok: true, value: '2026-03-29' });
    expect(readTyped('date', '31.04.2026', RU)).toEqual({ ok: false, problem: 'date' });
  });

  it('reads a date-time in the viewer’s zone', () => {
    expect(readTyped('date_time', '29.03.2026 15:15', RU)).toMatchObject({
      ok: true,
      value: '2026-03-29T12:15:00.000Z',
    });
    const entry = field(1, 'Deadline', 'date_time', { date_time: '2026-03-29T12:15:00Z' });
    expect(typedText(entry, RU)).toBe('29.03.2026 15:15');
  });
});

describe('FLD-007: periods', () => {
  it('reads periods into minutes in either language', () => {
    expect(readTyped('duration', '1h 30m', EN)).toMatchObject({ ok: true, value: 90 });
    expect(readTyped('duration', '1ч 30м', RU)).toMatchObject({ ok: true, value: 90 });
    expect(readTyped('duration', '30m 1h', EN)).toEqual({ ok: false, problem: 'duration' });
    expect(typedText(field(1, 'Estimation', 'duration', { minutes: 90 }), RU)).toBe('1ч 30м');
  });
});

describe('FLD-002: strings', () => {
  it('keeps the text and clears the field when it is empty', () => {
    expect(readTyped('string', 'Chrome 130', EN)).toEqual({
      ok: true,
      value: 'Chrome 130',
      shown: { text: 'Chrome 130' },
    });
    expect(readTyped('string', '  ', EN)).toMatchObject({ ok: true, value: null });
  });
});

describe('ISS-019: choices', () => {
  it('sends one ID for single fields, IDs for multi-valued ones, null for none', () => {
    const single = field(1, 'Assignee', 'user', { users: [ANNA] });
    const multi = field(2, 'Reviewers', 'user', {});
    multi.field.multi = true;
    const choices = [
      { kind: 'user' as const, user: ANNA },
      { kind: 'user' as const, user: BORIS },
    ];
    expect(toInput(single, choices.slice(1))).toBe(2);
    expect(toInput(multi, choices)).toEqual([1, 2]);
    expect(toInput(single, [])).toBeNull();
    expect(heldChoices(withChoices(multi, choices))).toEqual(choices);
  });
});
