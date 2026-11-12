import { afterEach, describe, expect, it } from 'vitest';

import { BASE_LOCALE, detectLocale, getLocale, setLocale } from './locale.svelte.ts';
import { formatNumber, msg, plural } from './message.ts';
import { errorText } from './messages/errors.ts';

afterEach(() => {
  setLocale(BASE_LOCALE);
});

describe('detectLocale', () => {
  it('prefers a stored choice, then browser languages, then English', () => {
    expect(detectLocale('ru', ['en-US'])).toBe('ru');
    expect(detectLocale(null, ['de-DE', 'ru-RU', 'en'])).toBe('ru');
    expect(detectLocale('xx', ['EN-gb'])).toBe('en');
    expect(detectLocale(null, ['de', 'fr'])).toBe(BASE_LOCALE);
  });
});

describe('msg', () => {
  const greeting = msg<{ name: string }>({
    en: ({ name }) => `Hello, ${name}`,
    ru: ({ name }) => `Привет, ${name}`,
  });
  const save = msg({ en: 'Save', ru: 'Сохранить' });

  it('renders the current language', () => {
    expect(save()).toBe('Save');
    expect(greeting({ name: 'Ann' })).toBe('Hello, Ann');
    setLocale('ru');
    expect(getLocale()).toBe('ru');
    expect(save()).toBe('Сохранить');
    expect(greeting({ name: 'Аня' })).toBe('Привет, Аня');
  });

  it('has a text for every API error code in every language', () => {
    for (const locale of ['en', 'ru'] as const) {
      setLocale(locale);
      for (const text of Object.values(errorText)) expect(text().length).toBeGreaterThan(5);
    }
  });
});

describe('plural', () => {
  const issues = (count: number) =>
    plural('ru', count, { one: '# задача', few: '# задачи', many: '# задач', other: '# задачи' });

  it('uses the Russian forms', () => {
    expect([1, 2, 5, 11, 21, 22, 25, 1.5].map(issues)).toEqual([
      '1 задача',
      '2 задачи',
      '5 задач',
      '11 задач',
      '21 задача',
      '22 задачи',
      '25 задач',
      '1,5 задачи',
    ]);
  });

  it('uses the English forms and groups digits', () => {
    const forms = { one: '# issue', other: '# issues' };
    expect(plural('en', 1, forms)).toBe('1 issue');
    expect(plural('en', 1200, forms)).toBe('1,200 issues');
    expect(formatNumber('ru', 1200)).toBe('1\u00a0200');
  });
});
