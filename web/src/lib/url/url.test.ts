import { describe, expect, it } from 'vitest';

import { readableSlug, withSearchParams } from './url.ts';

describe('UI-006: readable slugs', () => {
  it('keeps letters and digits of any script', () => {
    expect(readableSlug('Fix login on Safari 17.4!')).toBe('fix-login-on-safari-17-4');
    expect(readableSlug('Ошибка входа: «Сафари»')).toBe('ошибка-входа-сафари');
    expect(readableSlug('  ***  ')).toBe('');
  });

  it('cuts at a word boundary', () => {
    expect(readableSlug('one two three four', 10)).toBe('one-two');
    expect(readableSlug('Supercalifragilistic', 5)).toBe('super');
  });
});

describe('UI-006: view state in the URL', () => {
  it('sets, replaces and removes search parameters', () => {
    expect(withSearchParams('/issues?q=open&sort=updated', { q: 'state: done', page: null })).toBe(
      '/issues?q=state%3A+done&sort=updated',
    );
    expect(withSearchParams('/issues?q=open#top', { q: '' })).toBe('/issues#top');
    expect(withSearchParams(new URL('https://example.org/boards/3'), { sprint: '7' })).toBe(
      '/boards/3?sprint=7',
    );
  });
});
