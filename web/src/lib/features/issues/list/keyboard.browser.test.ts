// The list on the keyboard (step 2.2 §6, QRY-085, UI-022 list): roving tabindex — Tab from the
// query bar enters the first row, Esc returns to the bar — the arrows, Home and End move the focus
// with a polite announcement, `,` and `.` turn pages, Enter and `o` open (`Ctrl/⌘+Enter` in a new
// tab), `e` opens in edit mode, `s` stars, `m` assigns to the signed-in person, `b` hides the
// sidebar, and the page holds the `list` context with the focused row as the copy trio's entity.
// Every case runs on the desktop and the phone profile (D-019).
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

import type { Issue, IssueSearchPage } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { loadQueryEngine } from '#lib/query/index.ts';
import { active, focusedEntity, resetShortcutsForTests } from '#lib/shortcuts/registry.svelte.ts';
import {
  preferences,
  setAccessForTests,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { announcement } from '#lib/ui/announce.svelte.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { isPhone } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { resetCloning } from '../clone.svelte.ts';
import { ASSIGNEE, ESTIMATE, issue, PRIORITY } from '../test-data.ts';
import { forgetIssueFields } from './columns.ts';
import IssueList from './IssueList.svelte';
import type { ListParams, ResolvedContext } from './list-state.svelte.ts';
import { ensureSearch, PAGE } from './search.svelte.ts';

const opened = vi.hoisted(() => ({ window: vi.fn() }));

/** The signed-in person the session says; `m` makes her the assignee. */
const session = {
  user: {
    id: 7,
    username: 'anna',
    full_name: 'Anna Admin',
    email: null,
    email_verified: false,
    avatar_color: 3,
    avatar_url: null,
    version: 4,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

/** The row the list starts with: its assignee field is the builtin one `m` writes. */
function loginRow(): Issue {
  return issue({
    fields: [
      PRIORITY,
      { ...ASSIGNEE, field: { ...ASSIGNEE.field, builtin: 'assignee' } },
      ESTIMATE,
    ],
  });
}

const LOGIN = loginRow();
const SIGNUP = issue({ id: 502, readable_id: 'WEB-13', number: 13, summary: 'Signup is slow' });

function searchPage(
  items: Issue[],
  rest: Partial<IssueSearchPage> = {},
): { body: IssueSearchPage } {
  return {
    body: {
      items,
      next_cursor: null,
      sort: [],
      notes: [],
      query: '',
      single_id: null,
      live_topic: 'issues',
      ...rest,
    },
  };
}

/** One row with its ID, readable ID and summary in step (`501` is `WEB-12`). */
function row(id: number): Issue {
  return issue({
    id,
    number: id - 489,
    readable_id: `WEB-${String(id - 489)}`,
    summary: `Row ${String(id - 489)}`,
  });
}

/** `PAGE` rows, then one more as the next page the server would answer. */
function twoPages(): FakeRoute {
  return (request: FakeRequest) =>
    request.query.get('cursor')
      ? searchPage([row(501 + PAGE)])
      : searchPage(
          Array.from({ length: PAGE }, (_, i) => row(501 + i)),
          {
            next_cursor: 'next',
          },
        );
}

interface Shown {
  calls: FakeRequest[];
  onnavigate: ReturnType<typeof vi.fn<(params: ListParams) => void>>;
}

async function show(options: { search?: FakeRoute } = {}): Promise<Shown> {
  const calls = fakeApi({
    'PATCH /users/me/preferences': { body: {} },
    'GET /issues': options.search ?? searchPage([LOGIN, SIGNUP]),
    'GET /issues/count': { body: { all: 2, unresolved: 2 } },
    'GET /issues/fields': { body: { items: [PRIORITY, ASSIGNEE, ESTIMATE] } },
    'DELETE /issues/501/star': { status: 204 },
    'PUT /issues/501/fields/12': { body: { issue: LOGIN, messages: [] } },
  });
  setSessionForTests(session);
  setPreferencesForTests({});
  const context: ResolvedContext = { kind: 'all' };
  const list = await ensureSearch({
    q: '',
    context,
    hideResolved: preferences.value('list_hide_resolved'),
  });
  await list.refresh();
  const onnavigate = vi.fn<(params: ListParams) => void>();
  await render(IssueList, { props: { q: '', context, list, error: null, onnavigate } });
  return { calls, onnavigate };
}

/** The list's rows: the table's `<tr>` on a desktop, the cards' `<li>` on a phone. */
function rows(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>('[data-row]')];
}

function focusedRow(): string | null {
  const activeElement = document.activeElement;
  return activeElement instanceof HTMLElement ? (activeElement.dataset.row ?? null) : null;
}

/** Presses on the row in focus: the shared handler listens on the row itself. */
async function press(keys: string): Promise<void> {
  await userEvent.keyboard(keys);
}

beforeAll(async () => {
  // The language loads before `fetch` is replaced by the fake.
  await loadQueryEngine();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  store.entities.table('issue').clear();
  forgetIssueFields();
  resetCloning();
  resetShortcutsForTests();
  setSessionForTests(null);
  setAccessForTests(null);
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('The list on the keyboard', () => {
  it('QRY-085: Tab from the query bar enters the first row, Esc returns to the bar', async () => {
    await show();
    const input = page.getByRole('combobox', { name: 'Search query' });
    await userEvent.click(input);

    // Roving tabindex: the first row is the one tab stop of the list (§6).
    await press('{Tab}');
    expect(focusedRow()).toBe('501');
    expect(rows()[0]?.tabIndex).toBe(0);
    expect(rows()[1]?.tabIndex).toBe(-1);

    await press('{Escape}');
    expect(focusedRow()).toBeNull();
    expect(document.activeElement).toBe(input.element());

    // Tab again re-enters at the first row, not the last focused one.
    await press('{Tab}');
    expect(focusedRow()).toBe('501');
  });

  it('QRY-085: arrows, Home and End move the focus and announce the row', async () => {
    await show();
    await userEvent.click(page.getByRole('combobox', { name: 'Search query' }));
    await press('{Tab}');

    await press('{ArrowDown}');
    expect(focusedRow()).toBe('502');
    expect(announcement('polite')).toContain('WEB-13 Signup is slow');

    await press('{ArrowUp}');
    expect(focusedRow()).toBe('501');

    await press('{End}');
    expect(focusedRow()).toBe('502');

    await press('{Home}');
    expect(focusedRow()).toBe('501');

    // The move stops at the ends.
    await press('{ArrowUp}');
    expect(focusedRow()).toBe('501');
  });

  it('QRY-085, UI-022: Enter and `o` open the focused issue, Ctrl/⌘+Enter in a new tab', async () => {
    vi.stubGlobal('open', opened.window);
    await show();
    await userEvent.click(page.getByRole('combobox', { name: 'Search query' }));
    await press('{Tab}{ArrowDown}');

    await press('{Enter}');
    expect(navigation.goto).toHaveBeenCalledWith('/issues/WEB-13');

    await press('o');
    expect(navigation.goto).toHaveBeenCalledTimes(2);

    navigation.goto.mockClear();
    await press('{Control>}{Enter}{/Control}');
    expect(opened.window).toHaveBeenCalledWith('/issues/WEB-13', '_blank');
    expect(navigation.goto).not.toHaveBeenCalled();
  });

  it('QRY-085: `e` opens the focused issue in edit mode', async () => {
    await show();
    await userEvent.click(page.getByRole('combobox', { name: 'Search query' }));
    await press('{Tab}');

    await press('e');
    expect(navigation.goto).toHaveBeenCalledWith('/issues/WEB-12?edit');
  });

  it('QRY-085: `s` stars the focused row, `m` assigns it to the signed-in person', async () => {
    const { calls } = await show();
    // The focused row starts starred: `s` takes the star off.
    await userEvent.click(page.getByRole('combobox', { name: 'Search query' }));
    await press('{Tab}');

    await press('s');
    expect(
      calls.find((call) => call.method === 'DELETE' && call.path === '/issues/501/star'),
    ).toBeDefined();

    await press('m');
    const assigned = calls.find(
      (call) => call.method === 'PUT' && call.path === '/issues/501/fields/12',
    );
    expect(assigned?.body).toEqual({ value: 7 });
    await expect.poll(() => announcement('polite')).toContain('Assigned to me');
  });

  it('QRY-085: `b` hides the sidebar (the drawer on a phone)', async () => {
    const { calls } = await show();
    await userEvent.click(page.getByRole('combobox', { name: 'Search query' }));
    await press('{Tab}');

    await press('b');
    if (isPhone()) {
      await expect.element(page.getByRole('dialog', { name: 'Lists' })).toBeVisible();
    } else {
      // The preference save is debounced; the call lands shortly after the press.
      await expect
        .poll(
          () =>
            calls.find((call) => call.method === 'PATCH' && call.path === '/users/me/preferences')
              ?.body,
        )
        .toMatchObject({ list_sidebar: 'closed' });
    }
  });

  it('QRY-081, QRY-085: `,` and `.` turn pages, loading the next one when the step needs it', async () => {
    await show({ search: twoPages() });
    expect(rows()).toHaveLength(PAGE);
    await userEvent.click(page.getByRole('combobox', { name: 'Search query' }));
    await press('{Tab}');

    // One page forward: the page after the loaded one loads, and its first row takes the focus.
    await press('.');
    await expect.element(page.getByText(`Row ${String(PAGE + 12)}`)).toBeVisible();
    await expect.poll(() => focusedRow()).toBe(String(501 + PAGE));
    expect(announcement('polite')).toContain(`WEB-${String(PAGE + 12)} Row ${String(PAGE + 12)}`);

    // No page after the last one: the focus stays.
    await press('.');
    expect(focusedRow()).toBe(String(501 + PAGE));

    await press(',');
    expect(focusedRow()).toBe('501');

    // No page before the first one.
    await press(',');
    expect(focusedRow()).toBe('501');
  });

  it('UI-024, QRY-085: the page holds the list context and the focused row is the copy trio’s entity', async () => {
    await show();
    expect(active()).toBe('list');
    expect(focusedEntity()).toBeNull();

    await userEvent.click(page.getByRole('combobox', { name: 'Search query' }));
    await press('{Tab}{ArrowDown}');
    expect(focusedEntity()).toEqual({
      id: 'WEB-13',
      href: '/issues/WEB-13',
      summary: 'Signup is slow',
    });
  });

  it('QRY-085: letters keep typing in the query bar — no row action fires there', async () => {
    const { calls } = await show();
    const input = page.getByRole('combobox', { name: 'Search query' });
    await userEvent.click(input);
    await press('s');
    expect((input.element() as HTMLInputElement).value).toBe('s');
    expect(calls.find((call) => call.path.includes('/star'))).toBeUndefined();
  });
});
