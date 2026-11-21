// Feedback toasts: confirmations, errors with retry, undo; announced to screen readers (UI-038).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import { ApiError } from '#lib/api/client.ts';

import { expectAccessible, isPhone, smallTargets } from '../../test/a11y.ts';
import Announcer from './Announcer.svelte';
import { copyLink, copyText } from './copy.ts';
import {
  DEFAULT_DURATION,
  ERROR_DURATION,
  MAX_TOASTS,
  UNDO_DURATION,
  showError,
  toasts,
} from './toast.svelte.ts';
import Toaster from './Toaster.svelte';

async function setup() {
  await render(Announcer);
  return render(Toaster);
}

afterEach(() => {
  toasts.clear();
  vi.useRealTimers();
});

describe('toasts', () => {
  it('UI-038: confirms an action and announces it politely', async () => {
    await setup();
    toasts.show({ tone: 'success', message: 'Comment posted' });

    const region = page.getByRole('region', { name: 'Status messages' });
    await expect.element(region.getByText('Comment posted')).toBeVisible();
    await expect.element(page.getByTestId('announcer-polite')).toHaveTextContent('Comment posted');
    await expectAccessible(region.element());
  });

  it('UI-038: offers undo within the window, then the action is final', async () => {
    await setup();
    const undo = vi.fn();
    const onClose = vi.fn();
    toasts.show({ message: 'Removed from the board', undo, onClose });

    await page.getByRole('button', { name: 'Undo' }).click();
    expect(undo).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledWith('undone');
    await expect.element(page.getByText('Removed from the board')).not.toBeInTheDocument();

    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'performance'] });
    const final = vi.fn();
    toasts.show({ message: 'Removed again', undo, onClose: final });
    vi.advanceTimersByTime(UNDO_DURATION - 100);
    expect(final).not.toHaveBeenCalled();
    vi.advanceTimersByTime(200);
    expect(final).toHaveBeenCalledWith('timeout');
  });

  it('UI-038: errors hold the screen longest, with details and Try again', async () => {
    await setup();
    const retry = vi.fn();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'performance'] });
    showError(new ApiError(409, { code: 'conflict', message: 'stale' }, 'req-7'), retry);
    // Longer than every other toast, but not until dismissed.
    vi.advanceTimersByTime(DEFAULT_DURATION + 2000);
    expect(toasts.items).toHaveLength(1);
    // Matchers below poll with timers, so the real ones must be back.
    vi.useRealTimers();

    await expect
      .element(page.getByTestId('announcer-assertive'))
      .toMatchTextContent(/^Someone changed this/);
    await expect.element(page.getByText('Request ID: req-7')).toBeVisible();
    if (isPhone()) expect(await smallTargets(page.getByRole('region').element())).toEqual([]);

    await page.getByRole('button', { name: 'Try again' }).click();
    expect(retry).toHaveBeenCalledOnce();
    await expect.element(page.getByText('Request ID: req-7')).not.toBeInTheDocument();
  });

  it('UI-038: an error closes by itself after its longer window', async () => {
    await setup();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'performance'] });
    showError(new ApiError(409, { code: 'conflict', message: 'stale' }, 'req-7'));
    vi.advanceTimersByTime(ERROR_DURATION - 10);
    expect(toasts.items).toHaveLength(1);
    vi.advanceTimersByTime(20);
    expect(toasts.items).toHaveLength(0);
    vi.useRealTimers();
  });

  it('pauses while the pointer is over the toasts (WCAG 2.2.1)', async () => {
    await setup();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'performance'] });
    const onClose = vi.fn();
    toasts.show({ message: 'Saved', onClose });

    toasts.pause();
    vi.advanceTimersByTime(DEFAULT_DURATION * 2);
    expect(onClose).not.toHaveBeenCalled();
    toasts.resume();
    vi.advanceTimersByTime(DEFAULT_DURATION + 10);
    expect(onClose).toHaveBeenCalledWith('timeout');
  });

  it('shows at most a few at once, replacing the oldest', async () => {
    await setup();
    const first = vi.fn();
    toasts.show({ message: 'Toast 0', onClose: first });
    for (let index = 1; index <= MAX_TOASTS; index += 1)
      toasts.show({ message: `Toast ${String(index)}` });

    expect(toasts.items).toHaveLength(MAX_TOASTS);
    expect(first).toHaveBeenCalledWith('replaced');
    await expect.element(page.getByText('Toast 0')).not.toBeInTheDocument();
  });

  it('UI-006: copies a link and confirms it, or asks to copy by hand', async () => {
    await setup();
    const writeText = vi
      .spyOn(navigator.clipboard, 'writeText')
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error('denied'));

    await expect(copyLink('/issues?q=open')).resolves.toBe(true);
    expect(writeText).toHaveBeenCalledWith(`${location.origin}/issues?q=open`);
    await expect.element(page.getByText('Link copied')).toBeVisible();

    await expect(copyText('CORE-1')).resolves.toBe(false);
    await expect.element(page.getByText('CORE-1')).toBeVisible();
    writeText.mockRestore();
  });
});
