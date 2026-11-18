// The shortcut reference (UI-020, step 2.2 §5) and the global wiring of part 2.2a-1 (§4): `?`
// opens the overlay from the shell, the account menu opens it too, the global bindings and the
// active context's are shown grouped and filtered, the platform switch changes the glyphs, and
// `?` never fires while typing. Every case runs on the desktop and the phone profile (D-019).
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { labels } from '#lib/shortcuts/messages.ts';
import { detectPlatform, modifierGlyph } from '#lib/shortcuts/platform.ts';
import { claim, enter, resetShortcutsForTests } from '#lib/shortcuts/registry.svelte.ts';
import {
  setAccessForTests,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';

import { fakeApi } from '../../test/fake-api.ts';
import AppShell from './AppShell.svelte';
import * as m from './messages.ts';

const content = createRawSnippet(() => ({ render: () => '<h1>Page title</h1>' }));
const typing = createRawSnippet(() => ({ render: () => '<input aria-label="Note" />' }));

const session = {
  user: {
    id: 7,
    username: 'anna',
    full_name: 'Anna Admin',
    email: 'anna@example.com',
    email_verified: false,
    avatar_color: 3,
    avatar_url: null,
    version: 4,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

function label(id: keyof typeof labels): string {
  return labels[id]();
}

/** Presses the overlay's physical key — Shift and the `/` position — as a real keydown: typing
 * the character alone would only insert text, and the framework listens to keys, not glyphs. */
async function pressOverlayKey(): Promise<void> {
  await userEvent.keyboard('{Shift>}{Slash}{/Shift}');
}

/** Leaves the overlay closed for the next case: an open dialog owns the keys (§3) and would
 * swallow the next test's presses. */
async function closeOverlay(): Promise<void> {
  await userEvent.keyboard('{Escape}');
}

afterEach(() => {
  vi.unstubAllGlobals();
  resetShortcutsForTests();
  setSessionForTests(null);
  setAccessForTests(null);
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
  vi.clearAllMocks();
});

describe('ShortcutsOverlay', () => {
  it('UI-020: ? opens the reference with the global bindings and the caption', async () => {
    await render(AppShell, { pathname: '/issues', children: content });
    await pressOverlayKey();
    const dialog = page.getByRole('dialog', { name: m.shortcutsTitle() });
    await expect.element(dialog).toBeVisible();

    // The global rows, grouped by purpose, with the physical keys as chips.
    await expect.element(dialog.getByText(label('go.issues'))).toBeVisible();
    await expect.element(dialog.getByText(label('copy.id'))).toBeVisible();
    await expect.element(dialog.getByText('?', { exact: true })).toBeVisible();
    await expect.element(dialog.getByText(m.shortcutsCaption())).toBeVisible();
    await closeOverlay();
  });

  it('UI-020: shows the active context beside the global bindings', async () => {
    enter('list');
    await render(AppShell, { pathname: '/issues', children: content });
    await pressOverlayKey();
    const dialog = page.getByRole('dialog', { name: m.shortcutsTitle() });
    await expect.element(dialog.getByText(label('list.edit'))).toBeVisible();
    await expect.element(dialog.getByText(label('list.select'))).toBeVisible();
    // A context the page did not enter stays out.
    await expect.element(dialog.getByText(label('editor.bold'))).not.toBeInTheDocument();
    await closeOverlay();
  });

  it('UI-020: the filter narrows by label and by key', async () => {
    await render(AppShell, { pathname: '/issues', children: content });
    await pressOverlayKey();
    const dialog = page.getByRole('dialog', { name: m.shortcutsTitle() });
    const filter = dialog.getByLabelText(m.shortcutsFilterLabel());

    await filter.fill(label('copy.link'));
    await expect.element(dialog.getByText(label('copy.link'))).toBeVisible();
    await expect.element(dialog.getByText(label('copy.id'))).not.toBeInTheDocument();

    // By key: g names the go-to sequences.
    await filter.fill('g');
    await expect.element(dialog.getByText(label('go.boards'))).toBeVisible();
    await expect.element(dialog.getByText(label('copy.id'))).not.toBeInTheDocument();
    await closeOverlay();
  });

  it('UI-020: the platform switch changes the glyphs the card shows', async () => {
    await render(AppShell, { pathname: '/issues', children: content });
    await pressOverlayKey();
    const dialog = page.getByRole('dialog', { name: m.shortcutsTitle() });
    await expect.element(dialog).toBeVisible();
    const host = detectPlatform();
    const otherLabel = host === 'mac' ? m.shortcutsPlatformPc() : m.shortcutsPlatformMac();

    // The command row shows the chosen platform's primary modifier.
    const primary = modifierGlyph('primary', host === 'mac' ? 'pc' : 'mac');
    const before = [...dialog.element().querySelectorAll('kbd')];
    if (!before.some((kbd) => kbd.textContent === primary)) {
      await dialog.getByRole('radio', { name: otherLabel }).click();
    }
    await expect.element(dialog.getByText(primary, { exact: true }).first()).toBeVisible();
    await closeOverlay();
  });

  it('UI-020: the account menu opens the reference', async () => {
    setSessionForTests(session);
    fakeApi({
      'GET /notifications/summary': { body: { total: 0, direct: 0, subscriptions: 0, system: 0 } },
    });
    const screen = await render(AppShell, { pathname: '/issues', children: content });

    await screen.getByRole('button', { name: 'Account: Anna Admin' }).click();
    await page.getByRole('menuitem', { name: m.shortcutsTitle() }).click();
    await expect.element(page.getByRole('dialog', { name: m.shortcutsTitle() })).toBeVisible();
    await closeOverlay();
  });

  it('keeps ? from firing while typing', async () => {
    const screen = await render(AppShell, { pathname: '/issues', children: typing });
    await screen.getByLabelText('Note').click();
    await pressOverlayKey();
    await expect
      .element(page.getByRole('dialog', { name: m.shortcutsTitle() }))
      .not.toBeInTheDocument();
    // The typed character landed in the field instead.
    await expect.element(screen.getByLabelText('Note')).toHaveValue('?');
  });

  it('UI-023: the reference lists the query bar and the command dialog sections', async () => {
    await render(AppShell, { pathname: '/issues', children: content });

    enter('query');
    await pressOverlayKey();
    let dialog = page.getByRole('dialog', { name: m.shortcutsTitle() });
    await expect.element(dialog.getByText(label('query.suggest'))).toBeVisible();
    await expect.element(dialog.getByText(label('query.run'))).toBeVisible();
    await closeOverlay();

    enter('dialog');
    await pressOverlayKey();
    dialog = page.getByRole('dialog', { name: m.shortcutsTitle() });
    // Apply and Apply-with-⌘ share their label; one row of the pair is enough here.
    await expect.element(dialog.getByText(label('dialog.apply')).first()).toBeVisible();
    await expect.element(dialog.getByText(label('dialog.apply.silent'))).toBeVisible();
    await closeOverlay();
  });

  it('commands_on_typing: an unbound letter opens the claimed dialog prefilled', async () => {
    setPreferencesForTests({ commands_on_typing: true });
    const seen: (string | undefined)[] = [];
    const release = claim('command', (preset?: string) => {
      seen.push(preset);
      return true;
    });
    enter('issue');
    const screen = await render(AppShell, { pathname: '/issues', children: content });
    // Keys need the frame's focus: another file's iframe may hold it in a full run.
    await screen.getByRole('heading', { name: 'Page title' }).click();

    await userEvent.keyboard('q');
    await vi.waitFor(() => {
      expect(seen).toEqual(['q']);
    });

    // Letters bound in the active context keep their action: the dialog is not asked.
    await userEvent.keyboard('e');
    await userEvent.keyboard('s');
    expect(seen).toEqual(['q']);

    // The g sequence's leading key is not a stray letter either.
    await userEvent.keyboard('g');
    expect(seen).toEqual(['q']);
    release();
  });

  it('commands_on_typing stays off while typing', async () => {
    setPreferencesForTests({ commands_on_typing: true });
    let asked = 0;
    const release = claim('command', () => {
      asked += 1;
      return true;
    });
    const screen = await render(AppShell, { pathname: '/issues', children: typing });
    await screen.getByLabelText('Note').click();
    await userEvent.keyboard('q');
    await expect.element(screen.getByLabelText('Note')).toHaveValue('q');
    expect(asked).toBe(0);
    release();
  });

  it('goes to a section by the g sequence', async () => {
    await render(AppShell, { pathname: '/boards', children: content });
    await userEvent.keyboard('g');
    await userEvent.keyboard('i');
    await vi.waitFor(() => {
      expect(vi.mocked(navigation.goto)).toHaveBeenCalledWith('/issues');
    });
  });
});
