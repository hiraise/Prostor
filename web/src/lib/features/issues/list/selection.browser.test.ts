// The selection in the issue list and its toolbar (step 2.2 §8, QRY-083, QRY-084 without its
// pickers and export): the header checkbox chooses the page and the half-chosen page is mixed,
// «Выбрать все N задач по запросу» takes the query's worth, one unchecked row of it drops to the
// page's IDs, shift-click takes the range from the anchor, `x`, `Shift+↑/↓` and `Ctrl/⌘+A` drive
// the same model, the count is announced politely, the selection survives «Показать ещё», and the
// toolbar runs everything through the command engine — the chosen IDs directly, everything
// matched as the bulk job, «Удалить» with a confirm that names the count. Phones: «Выбрать» turns
// the selection mode on, the toolbar is a bottom sheet. Every case runs on both profiles (D-019).
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

import type {
  AccountSummary,
  CommandAssist,
  Issue,
  IssueSearchPage,
  LinkType,
  Tag,
} from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { loadQueryEngine } from '#lib/query/index.ts';
import { dispatchClaim, resetShortcutsForTests } from '#lib/shortcuts/registry.svelte.ts';
import {
  preferences,
  setAccessForTests,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { announcement } from '#lib/ui/announce.svelte.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { resetCloning } from '../clone.svelte.ts';
import { forgetLinkTypes } from '../links/links.svelte.ts';
import { issue, PRIORITY, ASSIGNEE, ESTIMATE } from '../test-data.ts';
import { forgetIssueFields } from './columns.ts';
import IssueList from './IssueList.svelte';
import type { ListParams, ResolvedContext } from './list-state.svelte.ts';
import { ensureSearch } from './search.svelte.ts';

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

/** A row that may be deleted and changed without notifications, as an author may her own. */
function row(id: number, overrides: Partial<Issue> = {}): Issue {
  return issue({
    id,
    number: id - 489,
    readable_id: `WEB-${String(id - 489)}`,
    summary: `Row ${String(id - 489)}`,
    can: {
      update: true,
      update_private: false,
      delete: true,
      manage_watchers: false,
      vote: true,
      silent: true,
    },
    ...overrides,
  });
}

const ROWS = [row(501), row(502), row(503)];

/** A tag the person may put on issues (ISS-035). */
function tag(id: number, name: string): Tag {
  return {
    id,
    name,
    color: 3,
    owner: {
      id: 7,
      username: 'anna',
      full_name: 'Anna Admin',
      avatar_color: 3,
      avatar_url: null,
      disabled: false,
      kind: 'person',
      pseudonym: null,
    },
    sharing: null,
    abilities: { apply: true, edit: true, delete: true, share: true, transfer: false },
    version: 4,
    created_at: '2026-10-20T09:00:00Z',
  };
}

/** An account the directory offers (QRY-057). */
function account(id: number, fullName: string, username: string): AccountSummary {
  return {
    id,
    full_name: fullName,
    username,
    avatar_color: id % 8,
    avatar_url: null,
    created_at: '2026-10-20T09:00:00Z',
    disabled: null,
    email: `${username}@example.com`,
    email_verified: true,
    full_profile: true,
    groups: [],
    projects: [],
    guest: false,
    invited: false,
    version: 1,
  };
}

/** The link types the person's instance holds (LNK-002). */
const RELATED: LinkType = {
  id: 1,
  name: 'Related',
  source_name: 'related to',
  target_name: null,
  direction: 'undirected',
  builtin: 'related',
  locked: false,
  deleted_at: null,
  usage: null,
  version: 1,
};
const DEPENDS: LinkType = {
  id: 2,
  name: 'Dependency',
  source_name: 'blocks',
  target_name: 'blocked by',
  direction: 'directed',
  builtin: 'dependency',
  locked: false,
  deleted_at: null,
  usage: null,
  version: 1,
};

/** A successful apply over explicit IDs. */
const applied = (ids: number[]): FakeRoute => ({
  body: {
    operations: [],
    problems: [],
    issues: ids.map((id) => ({ issue: id, ok: true, problem: null, messages: [] })),
  },
});

function searchPage(items: Issue[], next: string | null = null): { body: IssueSearchPage } {
  return {
    body: {
      items,
      next_cursor: next,
      sort: [],
      notes: [],
      query: '',
      single_id: null,
      live_topic: 'issues',
    },
  };
}

/** The assist's answer the fake server gives. */
function assist(answer: Partial<CommandAssist> = {}): FakeRoute {
  return {
    body: { operations: [], problems: [], suggestions: [], ...answer },
  };
}

interface Shown {
  calls: FakeRequest[];
}

async function show(
  options: {
    rows?: Issue[];
    /** What `/issues/count` answers — more than the rows is what makes the offer of §8.1. */
    total?: number;
    next?: string | null;
    commands?: Record<string, FakeRoute>;
  } = {},
): Promise<Shown> {
  const rows = options.rows ?? ROWS;
  const calls = fakeApi({
    'PATCH /users/me/preferences': { body: {} },
    'GET /issues': options.next
      ? (request: FakeRequest) =>
          request.query.get('cursor')
            ? searchPage([row(504)])
            : searchPage(rows, options.next ?? null)
      : searchPage(rows),
    'GET /issues/count': {
      body: { all: options.total ?? rows.length, unresolved: options.total ?? rows.length },
    },
    'GET /issues/fields': { body: { items: [PRIORITY, ASSIGNEE, ESTIMATE].map((f) => f.field) } },
    'POST /commands/assist': assist(),
    ...options.commands,
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
  return { calls };
}

/** A row's checkbox by its readable ID. */
function box(id: string) {
  return page.getByRole('checkbox', { name: `Select ${id}` });
}

/** The selection toolbar: the picker buttons live here, clear of the columns' sort headers. */
function bar() {
  return page.getByRole('region', { name: 'Selected issues' });
}

function headerBox() {
  return page.getByRole('checkbox', { name: 'Select all issues on the page' });
}

/** Opens the selection mode the phone's way (§8.1); on a desktop the checkboxes are already there. */
async function startSelecting(): Promise<void> {
  if (!isPhone()) return;
  await userEvent.click(page.getByRole('button', { name: 'List settings' }));
  await userEvent.click(page.getByRole('menuitem', { name: 'Select' }));
  // The settings menu's popover closes behind the click; the next click waits for it — an open
  // popover would take it (the bail condition the row keys use).
  await vi.waitFor(() => {
    expect(document.querySelector('dialog[open], :popover-open')).toBeNull();
  });
}

beforeAll(async () => {
  await loadQueryEngine();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  store.entities.table('issue').clear();
  forgetIssueFields();
  forgetLinkTypes();
  resetCloning();
  resetShortcutsForTests();
  setSessionForTests(null);
  setAccessForTests(null);
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('The list’s selection', () => {
  // The header checkbox belongs to the table; a phone chooses the page with `Ctrl/⌘+A` — the
  // keyboard test below covers that.
  it.skipIf(isPhone())(
    'QRY-083: the header checkbox chooses the page, the half-chosen page is mixed',
    async () => {
      await show();

      await userEvent.click(headerBox());
      await expect.element(page.getByText('3 issues selected')).toBeVisible();
      expect(announcement('polite')).toContain('3 issues selected');

      await userEvent.click(box('WEB-12'));
      await expect.element(page.getByText('2 issues selected')).toBeVisible();
      const mixed = document.querySelector<HTMLInputElement>('thead input[type="checkbox"]');
      expect(mixed?.indeterminate).toBe(true);
      expect(mixed?.checked).toBe(false);

      await userEvent.click(headerBox());
      await expect.element(page.getByText('3 issues selected')).toBeVisible();
    },
  );

  it('QRY-083: shift-click takes the range from the last toggled row', async () => {
    await show();
    await startSelecting();

    await userEvent.click(box('WEB-12'));
    // The table's rows are the page's order; the range runs to the last row.
    await userEvent.click(box('WEB-14'), { modifiers: ['Shift'] });
    await expect.element(page.getByText('3 issues selected')).toBeVisible();
    for (const id of ['WEB-12', 'WEB-13', 'WEB-14']) {
      await expect.element(box(id)).toBeChecked();
    }

    // A later toggle moves the anchor: the next shift-click reaches from there.
    await userEvent.click(box('WEB-13'));
    await expect.element(page.getByText('2 issues selected')).toBeVisible();
  });

  it('QRY-083: «Select all … of the query» takes the query’s worth; one unchecked row drops to the page’s IDs', async () => {
    await show({ total: 5 });
    await startSelecting();

    await userEvent.click(box('WEB-12'));
    // Two rows of five: the offer names what the query matches beyond the page.
    await userEvent.click(page.getByRole('button', { name: 'Select all 5 issues of the query' }));
    await expect.element(page.getByText('all 5 issues of the query')).toBeVisible();
    expect(announcement('polite')).toContain('5 issues selected');
    for (const id of ['WEB-12', 'WEB-13', 'WEB-14']) {
      await expect.element(box(id)).toBeChecked();
    }

    // Unchecking a row drops to the page's explicit IDs; the rest of the page stays chosen.
    await userEvent.click(box('WEB-13'));
    await expect.element(page.getByText('2 issues selected')).toBeVisible();
    await expect.element(box('WEB-12')).toBeChecked();
    await expect.element(box('WEB-14')).toBeChecked();
  });

  it('QRY-083: `x` toggles the focused row, `Shift+↓` extends, `Ctrl/⌘+A` chooses the page', async () => {
    await show();
    // On a phone the mode also shows the checkboxes the assertions read.
    await startSelecting();
    const query = page.getByRole('combobox', { name: 'Search query' });
    await userEvent.click(query);
    await userEvent.keyboard('{Tab}');

    await userEvent.keyboard('x');
    await expect.element(page.getByText('1 issue selected')).toBeVisible();
    expect(announcement('polite')).toContain('1 issue selected');

    await userEvent.keyboard('{Shift>}{ArrowDown}{ArrowDown}{/Shift}');
    await expect.element(page.getByText('3 issues selected')).toBeVisible();
    await expect.element(box('WEB-12')).toBeChecked();
    await expect.element(box('WEB-14')).toBeChecked();

    await userEvent.keyboard('x');
    await expect.element(page.getByText('2 issues selected')).toBeVisible();

    await userEvent.keyboard('{Control>}a{/Control}');
    await expect.element(page.getByText('3 issues selected')).toBeVisible();
  });

  it('QRY-083: the selection survives «Show more»', async () => {
    const { calls } = await show({ next: 'next' });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await expect.element(page.getByText('1 issue selected')).toBeVisible();

    await userEvent.click(page.getByRole('button', { name: 'Show more' }));
    await expect.element(page.getByText('Row 15')).toBeVisible();
    await expect.element(page.getByText('1 issue selected')).toBeVisible();
    await expect.element(box('WEB-12')).toBeChecked();
    expect(calls.filter((call) => call.path === '/issues').length).toBeGreaterThan(1);
  });

  it('QRY-083, UI-032: the choosing list stays accessible, with reachable targets on a phone', async () => {
    const { expectAccessible } = await import('../../../../test/a11y.ts');
    await show();
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await expectAccessible();
    if (isPhone()) {
      const sheet = document.querySelector('.sheet');
      if (sheet === null) throw new Error('the bottom sheet did not render');
      expect(smallTargets(sheet)).toEqual([]);
    }
  });
});

describe('The selection toolbar', () => {
  it('QRY-084: «Команды…» applies the line to the chosen IDs', async () => {
    const { calls } = await show({
      commands: {
        'POST /commands': {
          body: {
            operations: [],
            problems: [],
            issues: [
              { issue: 501, ok: true, problem: null, messages: [] },
              { issue: 502, ok: true, problem: null, messages: [] },
            ],
          },
        },
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await userEvent.click(box('WEB-13'));

    await userEvent.click(page.getByRole('button', { name: 'Commands…' }));
    const input = page.getByRole('combobox', { name: 'Command' });
    await expect.element(input).toBeVisible();
    await input.fill('priority High');
    // The assist and the apply both carry the chosen IDs (§8.2).
    await vi.waitFor(() => {
      const asked = calls.find(
        (call) => call.method === 'POST' && call.path === '/commands/assist',
      );
      expect(asked?.body).toMatchObject({ issues: [501, 502] });
    });
    await userEvent.click(page.getByRole('button', { name: 'Apply', exact: true }));
    await vi.waitFor(() => {
      const applied = calls.find((call) => call.method === 'POST' && call.path === '/commands');
      expect(applied?.body).toMatchObject({ text: 'priority High', issues: [501, 502] });
    });
  });

  it('QRY-084, CMD-026: with everything matched the command runs as the bulk job', async () => {
    const { calls } = await show({
      total: 5,
      commands: {
        'POST /commands/bulk': { body: { job: 9 } },
        'GET /commands/bulk/9': {
          body: {
            id: 9,
            state: 'finished',
            done: 5,
            total: 5,
            failed: 0,
            failures: [],
            cancel_requested: false,
            last_error: null,
          },
        },
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await userEvent.click(page.getByRole('button', { name: 'Select all 5 issues of the query' }));

    await userEvent.click(page.getByRole('button', { name: 'Commands…' }));
    await expect
      .element(page.getByRole('dialog', { name: 'Apply a command to 5 issues' }))
      .toBeVisible();
    const input = page.getByRole('combobox', { name: 'Command' });
    await input.fill('priority High');
    await userEvent.click(page.getByRole('button', { name: 'Apply', exact: true }));
    await vi.waitFor(() => {
      const queued = calls.find((call) => call.method === 'POST' && call.path === '/commands/bulk');
      expect(queued?.body).toMatchObject({ text: 'priority High', query: '' });
    });
    // The 2.1 progress view, now opened from the toolbar.
    await expect.element(page.getByText('Done: applied to 5')).toBeVisible();
  });

  it('QRY-084: «Удалить» asks with the count and applies the delete command', async () => {
    const { calls } = await show({
      commands: {
        'POST /commands/assist': assist({
          operations: [{ kind: 'delete', start: 0, end: 6, names: [], mode: null, removals: [] }],
        }),
        'POST /commands': {
          body: {
            operations: [],
            problems: [],
            issues: [
              { issue: 501, ok: true, problem: null, messages: [] },
              { issue: 502, ok: true, problem: null, messages: [] },
            ],
          },
        },
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await userEvent.click(box('WEB-13'));

    // The bar's «Удалить» opens the dialog on the selection, its line already `delete`.
    await userEvent.click(page.getByRole('button', { name: 'Delete', exact: true }));
    const dialog = page.getByRole('dialog', { name: 'Commands' });
    const input = dialog.getByRole('combobox', { name: 'Command' });
    await expect.element(input).toHaveValue('delete');
    // The confirm names the count (§8.2).
    await userEvent.click(dialog.getByRole('button', { name: 'Delete', exact: true }));
    await expect.element(dialog.getByRole('heading', { name: 'Delete 2 issues?' })).toBeVisible();
    await userEvent.click(dialog.getByRole('button', { name: 'Delete', exact: true }));
    await vi.waitFor(() => {
      const applied = calls.find((call) => call.method === 'POST' && call.path === '/commands');
      expect(applied?.body).toMatchObject({ text: 'delete', issues: [501, 502] });
    });
  });

  it('QRY-084: «Удалить» waits for the delete right on something chosen', async () => {
    await show({
      rows: [
        row(501, {
          can: {
            update: true,
            update_private: false,
            delete: false,
            manage_watchers: false,
            vote: true,
            silent: false,
          },
        }),
      ],
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await expect.element(page.getByText('1 issue selected')).toBeVisible();
    expect(page.getByRole('button', { name: 'Delete', exact: true }).elements()).toEqual([]);
    // The commands stay: the engine itself decides what it may do.
    await expect.element(page.getByRole('button', { name: 'Commands…' })).toBeVisible();
  });

  it('QRY-084: «Экспорт» sends the chosen issues, and the query when all-matching', async () => {
    const { calls } = await show({
      total: 5,
      commands: {
        'POST /issues/export': {
          body: '',
          headers: {
            'content-type': 'text/csv; charset=utf-8',
            'content-disposition': 'attachment; filename="issues.csv"',
            'x-exported-count': '2',
            'x-total-count': '5',
          },
        },
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await userEvent.click(box('WEB-13'));

    // The chosen IDs go as they are; fewer rows than matches is the note of §9.
    await userEvent.click(page.getByRole('button', { name: 'Export CSV', exact: true }));
    await vi.waitFor(() => {
      const asked = calls.find((call) => call.method === 'POST' && call.path === '/issues/export');
      expect(asked?.body).toMatchObject({ ids: [501, 502], format: 'csv' });
    });
    await vi.waitFor(() => {
      expect(toasts.items.map((toast) => toast.message)).toContain(
        'The file holds 2 of 5 issues — the export limit',
      );
    });

    // All-matching exports the query as the list runs it, in either format.
    await userEvent.click(page.getByRole('button', { name: 'Select all 5 issues of the query' }));
    await userEvent.click(page.getByRole('button', { name: 'Export XLSX', exact: true }));
    await vi.waitFor(() => {
      const asked = calls.filter(
        (call) => call.method === 'POST' && call.path === '/issues/export',
      );
      expect(asked.at(-1)?.body).toEqual({ query: {}, format: 'xlsx' });
    });
  });

  it('QRY-084: «Снять выделение» lets the selection go; the phone leaves its mode', async () => {
    await show();
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await expect.element(page.getByText('1 issue selected')).toBeVisible();
    await userEvent.click(page.getByRole('button', { name: 'Clear selection' }));
    expect(page.getByRole('region', { name: 'Selected issues' }).elements()).toEqual([]);
    if (isPhone()) {
      expect(box('WEB-12').elements()).toEqual([]);
      // The mode is off: the settings menu offers to select again.
      await userEvent.click(page.getByRole('button', { name: 'List settings' }));
      await expect.element(page.getByRole('menuitem', { name: 'Select' })).toBeVisible();
    }
  });

  it('UI-019, QRY-084: ⌘/Ctrl+Alt+C opens the dialog on the selection — on nothing chosen, on the query', async () => {
    await show();
    await startSelecting();
    await userEvent.click(box('WEB-12'));

    expect(dispatchClaim('command')).toBe(true);
    await expect.element(page.getByRole('dialog', { name: 'Commands' })).toBeVisible();
    await userEvent.click(page.getByRole('button', { name: 'Cancel' }));

    // Bare letters of `commands_on_typing` ride the selection too (§8.2)…
    expect(dispatchClaim('command', 'p')).toBe(true);
    await userEvent.click(page.getByRole('button', { name: 'Cancel' }));

    // …and on nothing chosen the letters wait for one, while the combo opens the query's worth.
    await userEvent.click(page.getByRole('button', { name: 'Clear selection' }));
    expect(dispatchClaim('command', 'p')).toBe(false);
    expect(dispatchClaim('command')).toBe(true);
    await expect
      .element(page.getByRole('dialog', { name: /^Apply a command to \d+ issues$/ }))
      .toBeVisible();
  });
});

describe('The selection toolbar’s pickers', () => {
  it('QRY-084: «Тег» previews the tag command over the chosen IDs', async () => {
    const { calls } = await show({
      commands: {
        'GET /tags': { body: { items: [tag(5, 'web')], next_cursor: null } },
        'POST /commands': applied([501, 502]),
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await userEvent.click(box('WEB-13'));

    await userEvent.click(bar().getByRole('button', { name: 'Tag', exact: true }));
    const picker = page.getByRole('dialog', { name: 'Add a tag to the chosen issues' });
    await userEvent.click(picker.getByRole('radio', { name: 'web', exact: true }));

    // The choice becomes the line, and the dialog previews it over the chosen IDs (§8.3).
    const dialog = page.getByRole('dialog', { name: 'Commands' });
    await expect.element(dialog.getByRole('combobox', { name: 'Command' })).toHaveValue('tag web');
    await userEvent.click(dialog.getByRole('button', { name: 'Apply', exact: true }));
    await vi.waitFor(() => {
      const call = calls.find((c) => c.method === 'POST' && c.path === '/commands');
      expect(call?.body).toMatchObject({ text: 'tag web', issues: [501, 502] });
    });
  });

  it('QRY-084: a tag name nobody has is created, then applied (ISS-034)', async () => {
    const { calls } = await show({
      commands: {
        'GET /tags': { body: { items: [], next_cursor: null } },
        'POST /tags': ({ body }: FakeRequest) => ({
          status: 201,
          body: tag(9, (body as { name: string }).name),
        }),
        'POST /commands': applied([501]),
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));

    await userEvent.click(bar().getByRole('button', { name: 'Tag', exact: true }));
    const picker = page.getByRole('dialog', { name: 'Add a tag to the chosen issues' });
    await userEvent.fill(picker.getByRole('searchbox', { name: 'Find a tag' }), 'fresh');
    await userEvent.click(picker.getByRole('button', { name: 'Create tag “fresh”' }));

    const dialog = page.getByRole('dialog', { name: 'Commands' });
    await expect
      .element(dialog.getByRole('combobox', { name: 'Command' }))
      .toHaveValue('tag fresh');
    await userEvent.click(dialog.getByRole('button', { name: 'Apply', exact: true }));
    await vi.waitFor(() => {
      const call = calls.find((c) => c.method === 'POST' && c.path === '/commands');
      expect(call?.body).toMatchObject({ text: 'tag fresh', issues: [501] });
    });
  });

  it('QRY-084: «Исполнитель» names the field and the account', async () => {
    const { calls } = await show({
      commands: {
        'GET /users': { body: { items: [account(2, 'Boris Petrov', 'boris')], next_cursor: null } },
        'POST /commands': applied([501, 502]),
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await userEvent.click(box('WEB-13'));

    await userEvent.click(bar().getByRole('button', { name: 'Assignee', exact: true }));
    const picker = page.getByRole('dialog', { name: 'Choose an assignee' });
    await userEvent.click(picker.getByRole('radio', { name: /Boris Petrov/ }));

    // The line names the field as the person sees it, and the account by its username (§4.2).
    const dialog = page.getByRole('dialog', { name: 'Commands' });
    await expect
      .element(dialog.getByRole('combobox', { name: 'Command' }))
      .toHaveValue('Assignee boris');
    await userEvent.click(dialog.getByRole('button', { name: 'Apply', exact: true }));
    await vi.waitFor(() => {
      const call = calls.find((c) => c.method === 'POST' && c.path === '/commands');
      expect(call?.body).toMatchObject({ text: 'Assignee boris', issues: [501, 502] });
    });
  });

  it('QRY-084: «Добавить связь» builds one link command per chosen target', async () => {
    const { calls } = await show({
      rows: [row(501), row(502), row(503), row(504)],
      commands: {
        'GET /link-types': { body: { items: [RELATED, DEPENDS] } },
        'POST /commands': applied([501, 502]),
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await userEvent.click(box('WEB-13'));

    await userEvent.click(bar().getByRole('button', { name: 'Add link', exact: true }));
    const picker = page.getByRole('dialog', { name: 'Link the chosen issues' });
    // The first link name is chosen; the issue search offers the page's rows.
    await expect.element(picker.getByRole('combobox', { name: 'Link' })).toHaveValue('1:source');
    await userEvent.fill(picker.getByRole('searchbox', { name: 'Find an issue' }), 'Row');
    // Targets outside the selection: every chosen issue links to each of them.
    await userEvent.click(picker.getByRole('checkbox', { name: /WEB-14/ }));
    await userEvent.click(picker.getByRole('checkbox', { name: /WEB-15/ }));
    await userEvent.click(picker.getByRole('button', { name: 'Link 2' }));

    // Every chosen issue gets a link to every target — one command per pair, one line (§8.3).
    const dialog = page.getByRole('dialog', { name: 'Commands' });
    await expect
      .element(dialog.getByRole('combobox', { name: 'Command' }))
      .toHaveValue('related to WEB-14 related to WEB-15');
    await userEvent.click(dialog.getByRole('button', { name: 'Apply', exact: true }));
    await vi.waitFor(() => {
      const call = calls.find((c) => c.method === 'POST' && c.path === '/commands');
      expect(call?.body).toMatchObject({
        text: 'related to WEB-14 related to WEB-15',
        issues: [501, 502],
      });
    });
  });

  it('QRY-084: with everything matched the picker’s line runs as the bulk job’s', async () => {
    const { calls } = await show({
      total: 5,
      commands: {
        'GET /tags': { body: { items: [tag(5, 'web')], next_cursor: null } },
        'POST /commands/bulk': { body: { job: 11 } },
        'GET /commands/bulk/11': {
          body: {
            id: 11,
            state: 'finished',
            done: 5,
            total: 5,
            failed: 0,
            failures: [],
            cancel_requested: false,
            last_error: null,
          },
        },
      },
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await userEvent.click(page.getByRole('button', { name: 'Select all 5 issues of the query' }));

    await userEvent.click(bar().getByRole('button', { name: 'Tag', exact: true }));
    const picker = page.getByRole('dialog', { name: 'Add a tag to the chosen issues' });
    await userEvent.click(picker.getByRole('radio', { name: 'web', exact: true }));

    const dialog = page.getByRole('dialog', { name: 'Apply a command to 5 issues' });
    await expect.element(dialog.getByRole('combobox', { name: 'Command' })).toHaveValue('tag web');
    await userEvent.click(dialog.getByRole('button', { name: 'Apply', exact: true }));
    await vi.waitFor(() => {
      const queued = calls.find((c) => c.method === 'POST' && c.path === '/commands/bulk');
      expect(queued?.body).toMatchObject({ text: 'tag web', query: '' });
    });
  });

  it('QRY-084: the pickers wait for the update right on something chosen', async () => {
    await show({
      rows: [
        row(501, {
          can: {
            update: false,
            update_private: false,
            delete: false,
            manage_watchers: false,
            vote: true,
            silent: false,
          },
        }),
      ],
    });
    await startSelecting();
    await userEvent.click(box('WEB-12'));
    await expect.element(page.getByText('1 issue selected')).toBeVisible();
    expect(bar().getByRole('button', { name: 'Tag', exact: true }).elements()).toEqual([]);
    expect(bar().getByRole('button', { name: 'Assignee', exact: true }).elements()).toEqual([]);
    expect(bar().getByRole('button', { name: 'Add link', exact: true }).elements()).toEqual([]);
    // The commands stay: the engine itself decides what it may do.
    await expect.element(page.getByRole('button', { name: 'Commands…' })).toBeVisible();
  });
});
