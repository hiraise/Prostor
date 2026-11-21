// The application shell renders in both themes and both languages, on a desktop and on a phone
// (step 0.4 "done when"; UI-001, UI-032, UI-033, UI-035).
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import { BASE_LOCALE, setLocale, type Locale } from '#lib/i18n/locale.svelte.ts';
import {
  onSessionEnd,
  preferences,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';
import { applyTheme, initTheme, setThemePreference, type Theme } from '#lib/theme/theme.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../test/a11y.ts';
import { fakeApi } from '../../test/fake-api.ts';
import AppShell from './AppShell.svelte';
import { missingFeatures } from './browser-support.ts';
import { linkState } from './sections.ts';

const content = createRawSnippet(() => ({ render: () => '<h1>Page title</h1>' }));

const LABELS: Record<Locale, { nav: string; issues: string; boards: string; settings: string }> = {
  en: {
    nav: 'Main navigation',
    issues: 'Issues',
    boards: 'Boards',
    settings: 'Interface settings',
  },
  ru: {
    nav: 'Основная навигация',
    issues: 'Задачи',
    boards: 'Доски',
    settings: 'Настройки интерфейса',
  },
};

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setLocale(BASE_LOCALE);
  setThemePreference('system');
  applyTheme('light');
  localStorage.clear();
  setPreferencesForTests(null);
});

describe('AppShell', () => {
  for (const theme of ['light', 'dark'] as Theme[]) {
    for (const locale of ['en', 'ru'] as Locale[]) {
      it(`UI-001: renders in the ${theme} theme in ${locale}`, async () => {
        applyTheme(theme);
        setLocale(locale);
        const labels = LABELS[locale];
        const screen = await render(AppShell, { pathname: '/issues', children: content });

        await expect.element(screen.getByRole('main')).toHaveTextContent('Page title');
        if (isPhone()) {
          await screen.getByRole('button', { name: labels.nav }).click();
        }
        const nav = page.getByRole('navigation', { name: labels.nav });
        await expect
          .element(nav.getByRole('link', { name: labels.issues }))
          .toHaveAttribute('aria-current', 'page');
        await expect
          .element(nav.getByRole('link', { name: labels.boards }))
          .not.toHaveAttribute('aria-current');
        expect(document.documentElement.lang).toBe(locale);
        expect(document.documentElement.dataset.theme).toBe(theme);
        await expectAccessible(document.body);
      });
    }
  }

  it('UI-032: starts with a skip link to the main content', async () => {
    const screen = await render(AppShell, { pathname: '/issues', children: content });
    await userEvent.tab();
    const skip = screen.getByRole('link', { name: 'Skip to main content' });
    await expect.element(skip).toHaveFocus();
    await expect.element(skip).toHaveAttribute('href', '#main');
  });

  it('UI-001: the rail collapses to icons and remembers it (desktop)', async () => {
    if (isPhone()) return;
    const screen = await render(AppShell, { pathname: '/boards', children: content });
    await screen.getByRole('button', { name: 'Collapse navigation' }).click();

    // Labels stay in the accessible names.
    await expect
      .element(screen.getByRole('link', { name: 'Boards' }))
      .toHaveAttribute('aria-current', 'page');
    await expect.element(screen.getByText('Boards')).toHaveClass('sr-only');
    expect(preferences.value('rail_collapsed')).toBe(true);

    await screen.getByRole('button', { name: 'Expand navigation' }).click();
    await expect.element(screen.getByText('Boards')).not.toHaveClass('sr-only');
  });

  it.skipIf(isPhone())('UI-001: the rail stands at the width the person chose', async () => {
    setPreferencesForTests({ rail_width: 300 });
    const screen = await render(AppShell, { pathname: '/issues', children: content });
    const shell = screen.container.querySelector<HTMLElement>('.shell');
    expect(shell?.style.getPropertyValue('--rail-expanded')).toBe('30rem');

    // The keyboard works beside the drag: focus the handle and jump to a limit.
    const handle = screen.getByRole('separator', { name: 'Width of the navigation panel' });
    await expect.element(handle).toHaveAttribute('aria-valuenow', '300');
    await handle.click();
    await userEvent.keyboard('{End}');
    expect(preferences.value('rail_width')).toBe(400);
    expect(shell?.style.getPropertyValue('--rail-expanded')).toBe('40rem');

    // Collapsed to icons the rail keeps its fixed width and the handle goes away.
    await screen.getByRole('button', { name: 'Collapse navigation' }).click();
    expect(
      screen.getByRole('separator', { name: 'Width of the navigation panel' }).query(),
    ).toBeNull();
    expect(preferences.value('rail_width')).toBe(400);
  });

  it.runIf(isPhone())('UI-001: the drawer on a phone has no width handle', async () => {
    const screen = await render(AppShell, { pathname: '/issues', children: content });
    await screen.getByRole('button', { name: 'Main navigation' }).click();
    const drawer = page.getByRole('dialog', { name: 'Main navigation' });
    await expect.element(drawer).toBeVisible();
    expect(page.getByRole('separator').query()).toBeNull();
  });

  it('UI-033: on a phone the navigation is a drawer with large touch targets', async () => {
    if (!isPhone()) return;
    const screen = await render(AppShell, { pathname: '/issues', children: content });
    expect(page.getByRole('navigation').elements()).toHaveLength(0);

    const menu = screen.getByRole('button', { name: 'Main navigation' });
    expect(await smallTargets(screen.container)).toEqual([]);
    await menu.click();
    const drawer = page.getByRole('dialog', { name: 'Main navigation' });
    await expect.element(drawer).toBeVisible();
    expect(await smallTargets(drawer.element())).toEqual([]);

    // Choosing a section closes the drawer.
    drawer
      .element()
      .querySelector<HTMLAnchorElement>('a[href="/boards"]')
      ?.addEventListener('click', (event) => {
        event.preventDefault();
      });
    await drawer.getByRole('link', { name: 'Boards' }).click();
    await expect.element(drawer).not.toBeInTheDocument();
    await expect.element(menu).toHaveFocus();
  });

  it('switches theme and language from the settings menu', async () => {
    // The root layout starts this; it keeps <html data-theme> in step with the preference.
    const stopTheme = initTheme();
    const screen = await render(AppShell, { pathname: '/issues', children: content });
    await screen.getByRole('button', { name: 'Interface settings' }).click();
    await page.getByRole('menuitemradio', { name: 'Dark' }).click();
    expect(document.documentElement.dataset.theme).toBe('dark');

    await screen.getByRole('button', { name: 'Interface settings' }).click();
    await page.getByRole('menuitemradio', { name: 'Русский' }).click();
    await expect
      .element(screen.getByRole('button', { name: 'Настройки интерфейса' }))
      .toBeVisible();
    expect(document.documentElement.lang).toBe('ru');
    stopTheme();
  });
});

describe('AccountMenu', () => {
  it('UI-003, AUTH-005: shows who is signed in and signs out', async () => {
    setSessionForTests({
      user: {
        id: 1,
        username: 'anna',
        full_name: 'Anna Admin',
        email: null,
        email_verified: false,
        avatar_color: 0,
        avatar_url: null,
        version: 1,
      },
      remember: false,
      created_at: '2026-10-22T09:00:00Z',
      password_change_required: false,
    });
    const calls = fakeApi({ 'DELETE /session': { status: 204 } });
    const ended = vi.fn();
    const stop = onSessionEnd(ended);
    const screen = await render(AppShell, { pathname: '/issues', children: content });

    const button = screen.getByRole('button', { name: 'Account: Anna Admin' });
    // Initials always; the name too where the rail has room.
    await expect.element(button).toBeVisible();
    expect(button.element().textContent.replace(/\s+/g, ' ').trim()).toBe(
      isPhone() ? 'AA' : 'AA Account: Anna Admin',
    );
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await button.click();
    await expect.element(page.getByRole('menuitem', { name: 'Account' })).toBeVisible();
    await expectAccessible(document.body);
    await page.getByRole('menuitem', { name: 'Sign out' }).click();

    await vi.waitFor(() => {
      expect(ended).toHaveBeenCalledWith('signed-out');
    });
    // The bell's follow loads the unread counts once (NTF-048), then signing out ends the session.
    expect(calls.map((call) => `${call.method} ${call.path}`)).toEqual([
      'GET /notifications/summary',
      'DELETE /session',
    ]);
    stop();
  });
});

describe('sections', () => {
  it('marks the current section and pages inside it', () => {
    expect(linkState('/issues', '/issues')).toBe('page');
    expect(linkState('/issues', '/issues/CORE-12')).toBe('true');
    expect(linkState('/issues', '/issues-archive')).toBeUndefined();
  });
});

describe('browser support', () => {
  it('UI-035: the test browser has every feature of the baseline', () => {
    expect(missingFeatures()).toEqual([]);
  });
});
