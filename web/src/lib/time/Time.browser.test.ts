import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import { setLocale } from '#lib/i18n/locale.svelte.ts';

import { setTimeZone } from './context.svelte.ts';
import Time from './Time.svelte';

const plain = (text: string | null | undefined): string | undefined =>
  text?.replace(/[\u00a0\u202f\u2009]/g, ' ');

function timeElement(container: HTMLElement): HTMLTimeElement {
  const element = container.querySelector('time');
  if (!element) throw new Error('no <time> rendered');
  return element;
}

describe('Time', () => {
  beforeEach(() => {
    setLocale('en');
    setTimeZone('Europe/Berlin');
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('UI-030: renders an instant in the viewer language and zone, and follows changes', async () => {
    const screen = await render(Time, { value: '2026-03-29T13:15:00Z', year: 'always' });
    const time = timeElement(screen.container);
    expect(plain(time.textContent)).toBe('Mar 29, 2026, 3:15 PM');
    expect(time.getAttribute('datetime')).toBe('2026-03-29T13:15:00.000Z');
    expect(plain(time.title)).toBe('Sunday, March 29, 2026 at 3:15 PM');

    setLocale('ru');
    flushSync();
    expect(plain(time.textContent)).toBe('29 мар. 2026 г., 15:15');

    setTimeZone('America/New_York');
    flushSync();
    expect(plain(time.textContent)).toBe('29 мар. 2026 г., 9:15');
  });

  it('renders a calendar date without shifting it', async () => {
    setTimeZone('Pacific/Pago_Pago');
    const screen = await render(Time, { value: '2026-03-29', kind: 'date', year: 'always' });
    const time = timeElement(screen.container);
    expect(time.textContent).toBe('Mar 29, 2026');
    expect(time.getAttribute('datetime')).toBe('2026-03-29');
  });

  it('updates relative phrases with the clock', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
    vi.setSystemTime(Date.UTC(2026, 9, 21, 12, 0));
    const screen = await render(Time, { value: '2026-10-21T11:55:00Z', kind: 'relative' });
    const time = timeElement(screen.container);
    expect(time.textContent).toBe('5 minutes ago');
    expect(plain(time.title)).toBe('Wednesday, October 21, 2026 at 1:55 PM');

    vi.advanceTimersByTime(60_000);
    flushSync();
    expect(time.textContent).toBe('6 minutes ago');
  });

  it('renders nothing for a value that is not a date', async () => {
    const screen = await render(Time, { value: 'soon' });
    expect(screen.container.querySelector('time')).toBeNull();
  });
});
