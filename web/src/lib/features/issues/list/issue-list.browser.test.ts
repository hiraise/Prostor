// The issue list on a desktop and on a phone (step 1.7c-1): rows, count and empty hints, the
// query bar, contexts, hide resolved, saving searches, the copied link and «Показать ещё» (1.7c-1);
// the table and the phone cards, header sort, inline actions and the column chooser (1.7c-2).
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  AccessMap,
  Issue,
  IssueSearchPage,
  Preferences,
  QueryProblem,
} from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { loadQueryEngine } from '#lib/query/index.ts';
import { preferences, setAccessForTests, setPreferencesForTests } from '#lib/session/index.ts';
import type { TopicHandlers } from '#lib/live/connection.ts';
import { store, type LiveSource } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { resetCloning } from '../clone.svelte.ts';
import { ASSIGNEE, BORIS, DUE, ESTIMATE, issue, PRIORITY, WEB } from '../test-data.ts';
import { newIssueDialog, openNewIssue } from '../new/open.svelte.ts';
import { forgetIssueFields } from './columns.ts';
import IssueList from './IssueList.svelte';
import type { ListParams, ResolvedContext } from './list-state.svelte.ts';
import { ensureSearch } from './search.svelte.ts';

const LOGIN = issue();
const SIGNUP = issue({ id: 502, readable_id: 'WEB-13', number: 13, summary: 'Signup is slow' });
const FOOTER = issue({ id: 503, readable_id: 'WEB-14', number: 14, summary: 'Footer typo' });

const FIELDS = [PRIORITY, ASSIGNEE, ESTIMATE, DUE].map((entry) => entry.field);

const SEARCH: ResolvedContext = {
  kind: 'search',
  id: 7,
  name: 'My open',
  query: 'assignee: me sort: updated',
  projectKey: null,
  editable: true,
  mine: true,
  version: 3,
};

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

interface Shown {
  screen: Awaited<ReturnType<typeof render>>;
  calls: FakeRequest[];
  onnavigate: ReturnType<typeof vi.fn<(params: ListParams) => void>>;
}

async function show(
  options: {
    q?: string;
    context?: ResolvedContext | null;
    search?: FakeRoute;
    count?: FakeRoute;
    routes?: Record<string, FakeRoute>;
    prefs?: Preferences;
    live?: LiveSource;
  } = {},
): Promise<Shown> {
  const calls = fakeApi({
    'PATCH /users/me/preferences': { body: {} },
    'GET /issues': options.search ?? searchPage([LOGIN, SIGNUP]),
    'GET /issues/count': options.count ?? { body: { all: 7, unresolved: 3 } },
    'GET /issues/fields': { body: { items: FIELDS } },
    ...options.routes,
  });
  setPreferencesForTests(options.prefs ?? {});
  const q = options.q ?? '';
  const context = options.context === undefined ? { kind: 'all' as const } : options.context;
  let error: unknown = null;
  const list = await ensureSearch({
    q,
    context: context ?? { kind: 'all' },
    hideResolved: preferences.value('list_hide_resolved'),
  }).catch((failure: unknown) => {
    error = failure;
    return null;
  });
  // Lists stay cached between tests.
  await list?.refresh().catch(() => undefined);
  const onnavigate = vi.fn<(params: ListParams) => void>();
  const screen = await render(IssueList, {
    props: { q, context, list, error, onnavigate, live: options.live },
  });
  return { screen, calls, onnavigate };
}

/** The rows: a table on a desktop, a list of cards on a phone. */
function rowsOf(screen: Shown['screen']) {
  // The rows are a keyboard grid on a desktop (step 2.2 §6), a list of cards on a phone.
  return screen.getByRole(isPhone() ? 'list' : 'grid', { name: 'Found issues' });
}

/** How many issue rows are shown (the table's header row left out). */
function rowCount(screen: Shown['screen']): number {
  const rows = rowsOf(screen);
  return isPhone()
    ? rows
        .getByRole('listitem')
        .elements()
        .filter((e) => e.parentElement === rows.element()).length
    : rows.getByRole('row').elements().length - 1;
}

function searches(calls: FakeRequest[]): FakeRequest[] {
  return calls.filter((c) => c.method === 'GET' && c.path === '/issues');
}

beforeAll(async () => {
  // The language loads before `fetch` is replaced by the fake.
  await loadQueryEngine();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  store.entities.table('issue').clear();
  forgetIssueFields();
  resetCloning();
  setAccessForTests(null);
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('QRY-073: rows, count and empty hints', () => {
  it('shows the found issues with the count above them', async () => {
    const { screen } = await show();
    const rows = rowsOf(screen);
    await expect.element(rows.getByRole('link', { name: /WEB-12/ })).toBeVisible();
    await expect.element(rows.getByText('Signup is slow')).toBeVisible();
    await expect.element(screen.getByText('7 issues')).toBeVisible();
    await expect.element(screen.getByRole('heading', { level: 1, name: 'Issues' })).toBeVisible();
    await expectAccessible(screen.container);
  });

  it('UI-032: a resolved summary is struck through and said in words, not only muted', async () => {
    const done = issue({
      id: 504,
      readable_id: 'WEB-15',
      number: 15,
      resolved_at: '2026-10-24T08:00:00Z',
    });
    const { screen } = await show({ search: searchPage([done]) });
    const link = screen.getByRole('link', { name: /WEB-15/ });
    await expect.element(link).toBeVisible();
    expect(link.element().querySelector('s')).not.toBeNull();
    expect(link.element().textContent).toContain('(resolved)');
  });

  it('suggests what to change when nothing is found, and shows resolved issues on request', async () => {
    const { screen, calls } = await show({
      q: 'nothing matches',
      search: searchPage([]),
      count: { body: { all: 2, unresolved: 0 } },
      prefs: { list_hide_resolved: true },
    });
    await expect.element(screen.getByRole('heading', { name: 'No issues found' })).toBeVisible();
    await expect.element(screen.getByText('Remove some conditions from the query.')).toBeVisible();
    await expect.element(screen.getByText(/Resolved issues are hidden/)).toBeVisible();
    await expect.element(screen.getByText('0 issues')).toBeVisible();
    await screen.getByRole('button', { name: 'Show resolved' }).click();
    expect(preferences.value('list_hide_resolved')).toBe(false);
    await expect.poll(() => searches(calls).at(-1)?.query.get('hide_resolved')).toBeNull();
    await expectAccessible(screen.container);
  });
});

describe('QRY-074: the query bar', () => {
  it('searches the typed text and clears it', async () => {
    const { screen, onnavigate } = await show();
    const input = screen.getByRole('combobox', { name: 'Search query' });
    await input.fill('login sort: updated');
    await screen.getByRole('button', { name: 'Find' }).click();
    expect(onnavigate).toHaveBeenLastCalledWith({
      q: 'login sort: updated',
      context: { kind: 'all' },
    });
    await screen.rerender({ q: 'login sort: updated' });
    await screen.getByRole('button', { name: 'Clear' }).click();
    expect(onnavigate).toHaveBeenLastCalledWith({ q: '', context: { kind: 'all' } });
    await expect.element(input).toHaveValue('');
  });

  it('highlights the query and underlines the problems the server found', async () => {
    const problems: QueryProblem[] = [
      { from: 0, to: 6, code: 'unknown_attribute', severity: 'error' },
    ];
    const { screen } = await show({
      q: 'colour: red',
      search: { status: 400, body: { code: 'invalid_query', message: 'x', problems } },
      count: { status: 400, body: { code: 'invalid_query', message: 'x', problems } },
    });
    await expect.element(screen.getByText('The query has mistakes')).toBeVisible();
    await expect.element(screen.getByText('“colour”: no such field')).toBeVisible();
    const input = screen.getByRole('combobox', { name: 'Search query' });
    await expect.element(input).toHaveAttribute('aria-invalid', 'true');
    await expect.element(input).toHaveAccessibleDescription(/no such field/);
    await input.click();
    await expect
      .poll(() => screen.container.querySelector('[data-problem="error"]')?.textContent)
      .toBe('colour');
    expect(screen.container.querySelector('[data-class="attribute"]')?.textContent).toBe('colour');
    // A syntax problem is underlined while typing, before any search.
    await input.fill('(open');
    await expect
      .poll(() => screen.container.querySelector('[data-problem="error"]')?.textContent)
      .toBeTruthy();
    expect(rowsOf(screen).query()).toBeNull();
  });
});

describe('QRY-053: contexts', () => {
  it('chooses a project, tag or saved search as the context', async () => {
    const { screen, onnavigate } = await show({
      routes: {
        'GET /projects': {
          body: { items: [{ id: 1, key: 'WEB', name: 'Web' }], next_cursor: null },
        },
        'GET /tags': { body: { items: [{ id: 5, name: 'later' }], next_cursor: null } },
        'GET /saved-searches': {
          body: { items: [{ id: 7, name: 'My open' }], next_cursor: null },
        },
      },
    });
    await screen.getByRole('button', { name: 'Search in: All issues' }).click();
    await expect.element(screen.getByRole('radio', { name: /My open/ })).toBeVisible();
    await expect.element(screen.getByRole('radio', { name: /All issues/ })).toBeChecked();
    await userEvent.keyboard('lat');
    await expect.element(screen.getByRole('radio', { name: /Web/ })).not.toBeInTheDocument();
    await screen.getByRole('radio', { name: /later/ }).click();
    expect(onnavigate).toHaveBeenLastCalledWith({ q: '', context: { kind: 'tag', id: 5 } });
  });

  it('searches within a saved search and keeps its conditions folded', async () => {
    const { screen, calls } = await show({ context: SEARCH });
    await expect.element(screen.getByRole('button', { name: 'Search in: My open' })).toBeVisible();
    expect(searches(calls).at(-1)?.query.get('saved_search')).toBe('7');
    const toggle = screen.getByRole('button', { name: 'Show conditions' });
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect.element(screen.getByText('assignee: me sort: updated')).not.toBeVisible();
    await toggle.click();
    await expect.element(screen.getByText('assignee: me sort: updated')).toBeVisible();
    await expectAccessible(screen.container);
  });

  it('UI-005, ISS-005: shows a project’s issues under its crumbs, and new issues start there', async () => {
    const { screen, calls } = await show({
      context: { kind: 'project', id: 1, key: 'WEB', name: 'Web' },
    });
    const crumbs = screen.getByRole('navigation', { name: 'Breadcrumbs' });
    await expect
      .element(crumbs.getByRole('link', { name: 'Web' }).first())
      .toHaveAttribute('href', '/projects/WEB');
    expect(searches(calls).at(-1)?.query.get('project')).toBe('1');
    if (!isPhone()) {
      openNewIssue();
      expect(newIssueDialog.target).toEqual({ project: 'WEB' });
      newIssueDialog.open = false;
    }
    await screen.unmount();
    if (!isPhone()) {
      openNewIssue();
      expect(newIssueDialog.target).toEqual({});
      newIssueDialog.open = false;
    }
  });

  it('says when the context is missing and lists all issues', async () => {
    const { screen } = await show({ context: null });
    await expect.element(screen.getByText(/does not exist or is hidden/)).toBeVisible();
    await expect.element(rowsOf(screen)).toBeVisible();
  });
});

describe('QRY-054: hide resolved and saved searches', () => {
  it('keeps the toggle in the preferences and shows the note when the query overrides it', async () => {
    const note: QueryProblem = { from: 0, to: 12, code: 'hide_resolved_ignored', severity: 'note' };
    const { screen, calls } = await show({
      q: 'is: resolved',
      search: (request) =>
        searchPage([LOGIN], {
          notes: request.query.get('hide_resolved') === 'true' ? [note] : [],
        }),
    });
    const toggle = screen.getByRole('button', { name: 'Hide resolved' });
    await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');
    await toggle.click();
    expect(preferences.value('list_hide_resolved')).toBe(true);
    await expect.element(toggle).toHaveAttribute('aria-pressed', 'true');
    await expect.poll(() => searches(calls).at(-1)?.query.get('hide_resolved')).toBe('true');
    await expect
      .element(
        screen.getByText(/the query asks about resolution, so resolved issues are not hidden/),
      )
      .toBeVisible();
    // The note keeps resolved issues in the count.
    await expect.element(screen.getByText('7 issues')).toBeVisible();
    await expect.poll(() => calls.some((c) => c.method === 'PATCH')).toBe(true);
  });

  it('saves the query with its context as a new search', async () => {
    const { screen, calls, onnavigate } = await show({
      q: 'login',
      context: { kind: 'project', id: 1, key: 'WEB', name: 'Web' },
      routes: {
        'POST /saved-searches': {
          status: 201,
          body: {
            id: 9,
            name: 'Login in Web',
            query: 'project: WEB login',
            project: null,
            owner: { id: 1 },
            abilities: { apply: false, edit: true, delete: true, share: true, transfer: false },
            version: 1,
          },
        },
      },
    });
    await screen.getByRole('button', { name: 'Save search' }).click();
    await screen.getByRole('button', { name: 'Save', exact: true }).click();
    await expect.element(screen.getByText('Name the search.')).toBeVisible();
    await screen.getByRole('textbox', { name: 'Name' }).fill('Login in Web');
    await screen.getByRole('button', { name: 'Save', exact: true }).click();
    await expect
      .poll(() => calls.find((c) => c.method === 'POST')?.body)
      .toEqual({ name: 'Login in Web', query: 'project: WEB login' });
    await expect
      .poll(() => onnavigate.mock.lastCall)
      .toEqual([{ q: '', context: { kind: 'search', id: 9 } }]);
  });

  it('updates an editable saved search with the refined query', async () => {
    const { screen, calls } = await show({
      q: 'is: open',
      context: SEARCH,
      routes: {
        'PATCH /saved-searches/7': {
          body: {
            ...SEARCH,
            query: 'x',
            project: null,
            owner: { id: 1 },
            abilities: { edit: true },
          },
        },
      },
    });
    await screen.getByRole('button', { name: 'Update search' }).click();
    await expect
      .poll(() => calls.find((c) => c.method === 'PATCH' && c.path === '/saved-searches/7')?.body)
      .toEqual({ query: 'assignee: me is: open sort: updated', version: 3 });
  });

  it('offers no update for a search the person may not edit', async () => {
    const { screen } = await show({ q: 'is: open', context: { ...SEARCH, editable: false } });
    await expect.element(screen.getByRole('button', { name: 'Save search' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Update search' }).query()).toBeNull();
  });
});

describe('QRY-069: the copied link', () => {
  it('writes out the saved search’s query instead of its ID', async () => {
    const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    const { screen } = await show({ q: 'login', context: SEARCH });
    await screen.getByRole('button', { name: 'Copy link' }).click();
    await expect.poll(() => writeText.mock.calls.length).toBe(1);
    const url = new URL(String(writeText.mock.calls[0]?.[0]));
    expect(url.pathname).toMatch(/\/issues$/);
    expect(url.searchParams.get('q')).toBe('assignee: me login sort: updated');
    expect(url.searchParams.has('search')).toBe(false);
  });

  it('keeps a project context and the typed text', async () => {
    const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    const { screen } = await show({
      q: 'login',
      context: { kind: 'project', id: 1, key: 'WEB', name: 'Web' },
    });
    await screen.getByRole('button', { name: 'Copy link' }).click();
    await expect.poll(() => writeText.mock.calls.length).toBe(1);
    const url = new URL(String(writeText.mock.calls[0]?.[0]));
    expect([...url.searchParams]).toEqual([
      ['project', 'WEB'],
      ['q', 'login'],
    ]);
  });
});

describe('QRY-081: «Show more»', () => {
  it('appends the next page by the cursor and keeps the count', async () => {
    const { screen, calls } = await show({
      q: 'paged',
      search: (request) =>
        request.query.get('cursor') === 'c1'
          ? searchPage([FOOTER])
          : searchPage([LOGIN, SIGNUP], { next_cursor: 'c1' }),
    });
    await expect.element(rowsOf(screen)).toBeVisible();
    expect(rowCount(screen)).toBe(2);
    await screen.getByRole('button', { name: 'Show more' }).click();
    await expect.poll(() => rowCount(screen)).toBe(3);
    expect(searches(calls).at(-1)?.query.get('limit')).toBe('50');
    await expect.element(screen.getByText('7 issues')).toBeVisible();
    expect(calls.filter((c) => c.path === '/issues/count')).toHaveLength(1);
    await expect.element(screen.getByRole('button', { name: 'Show more' })).not.toBeInTheDocument();
  });

  it('offers a retry when the rows fail to load', async () => {
    const { screen } = await show({ q: 'broken', search: failure(500, 'internal') });
    await expect.element(screen.getByText(/The issues did not load/)).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Try again' })).toBeVisible();
  });
});

describe('QRY-086: live rows', () => {
  it('updates a shown row in place and offers «Refresh» for other changes', async () => {
    const topics = new Map<string, TopicHandlers>();
    const live: LiveSource = {
      subscribe(topic, handlers) {
        topics.set(topic, handlers);
        return () => topics.delete(topic);
      },
    };
    const send = (id: number, version: number, change: 'created' | 'updated'): void => {
      topics.get('issues')?.event?.({
        id: version,
        change,
        entity: { kind: 'issue', id, version },
        patch: null,
        topics: ['issues'],
      });
    };
    let pages = 0;
    const { screen, calls } = await show({
      q: 'live',
      live,
      search: () => {
        pages += 1;
        return searchPage(pages > 2 ? [FOOTER, LOGIN, SIGNUP] : [LOGIN, SIGNUP]);
      },
      routes: {
        'POST /issues/batch': {
          body: { items: [{ ...LOGIN, version: LOGIN.version + 1, summary: 'Login fixed' }] },
        },
      },
    });
    await expect.element(rowsOf(screen)).toBeVisible();
    await expect.poll(() => topics.has('issues')).toBe(true);

    send(LOGIN.id, LOGIN.version + 1, 'updated');
    await expect.element(screen.getByText('Login fixed')).toBeVisible();
    const batches = calls.filter((c) => c.path === '/issues/batch');
    expect(batches).toHaveLength(1);
    expect(batches[0]?.body).toEqual({ ids: [LOGIN.id] });
    await expect.element(screen.getByText(/The list has changed/)).not.toBeInTheDocument();

    send(FOOTER.id, 1, 'created');
    await expect.element(screen.getByText(/The list has changed/)).toBeVisible();
    expect(rowCount(screen)).toBe(2);
    const counted = calls.filter((c) => c.path === '/issues/count').length;
    await screen.getByRole('button', { name: 'Refresh' }).click();
    await expect.poll(() => rowCount(screen)).toBe(3);
    await expect.element(screen.getByText(/The list has changed/)).not.toBeInTheDocument();
    expect(calls.filter((c) => c.path === '/issues/count').length).toBe(counted + 1);
    await expectAccessible(screen.container);
  });
});

describe('QRY-075: the table, its columns and header sort', () => {
  it('shows ID, summary and the three most used fields when no columns are chosen', async () => {
    const { screen } = await show();
    const rows = rowsOf(screen);
    if (isPhone()) {
      await expect.element(rows.getByText('Major').first()).toBeVisible();
      await expect.element(rows.getByText(/1h\s30m/).first()).toBeVisible();
    } else {
      for (const name of ['ID', 'Summary', 'Priority', 'Assignee', 'Estimation'])
        await expect.element(rows.getByRole('columnheader', { name })).toBeVisible();
      expect(rows.getByRole('columnheader', { name: 'Due date' }).query()).toBeNull();
    }
    await expectAccessible(screen.container);
  });

  it('shows the columns of the preferences', async () => {
    const { screen } = await show({ prefs: { list_columns: ['summary', 'field:14', 'votes'] } });
    const rows = rowsOf(screen);
    await expect.element(rows.getByText('Login button does nothing')).toBeVisible();
    if (!isPhone()) {
      // The first columns are the selection checkbox and the star, the last the row's actions
      // (AUT-066) — all structural.
      const headers = () =>
        rows
          .getByRole('columnheader')
          .elements()
          .slice(2, -1)
          .map((e) => e.textContent.trim());
      await expect.poll(headers).toEqual(['Summary', 'Due date', 'Votes']);
    }
    expect(rows.getByText('Major').query()).toBeNull();
  });

  it.skipIf(isPhone())('sorts by a header: ascending, then descending', async () => {
    const { screen, onnavigate } = await show({ q: 'login sort: updated desc' });
    await rowsOf(screen).getByRole('button', { name: 'Summary' }).click();
    await expect.poll(() => onnavigate.mock.calls.at(-1)?.[0].q).toBe('login sort: summary asc');
  });

  it.skipIf(isPhone())('says the order in use and reverses it on a click', async () => {
    const { screen, onnavigate } = await show({
      q: 'sort: Priority asc',
      search: searchPage([LOGIN], {
        sort: [{ key: 'field', field: 11, direction: 'asc' }],
      }),
    });
    const header = rowsOf(screen).getByRole('columnheader', { name: 'Priority' });
    await expect.element(header).toHaveAttribute('aria-sort', 'ascending');
    await header.getByRole('button').click();
    await expect.poll(() => onnavigate.mock.calls.at(-1)?.[0].q).toBe('sort: Priority desc');
  });
});

describe('QRY-076: actions in the rows', () => {
  async function openActions(screen: Shown['screen']): Promise<void> {
    if (isPhone()) await screen.getByRole('button', { name: 'More for WEB-12' }).click();
  }

  it('toggles the star', async () => {
    const { screen, calls } = await show({
      routes: { 'DELETE /issues/501/star': { status: 204 } },
    });
    await openActions(screen);
    const star = screen.getByRole('button', { name: 'Watch WEB-12' });
    await expect.element(star).toHaveAttribute('aria-pressed', 'true');
    await star.click();
    await expect.element(star).toHaveAttribute('aria-pressed', 'false');
    expect(calls.some((c) => c.method === 'DELETE' && c.path === '/issues/501/star')).toBe(true);
  });

  it('changes a field in its cell', async () => {
    const { screen, calls } = await show({
      routes: {
        'PUT /issues/501/fields/13': () => ({ body: store.entities.table('issue').get(501) }),
      },
    });
    await openActions(screen);
    await screen.getByRole('button', { name: /Estimation of WEB-12/ }).click();
    const input = screen.getByRole('textbox', { name: 'Estimation' });
    await userEvent.fill(input, '1d 2h');
    await userEvent.keyboard('{Enter}');
    await expect
      .element(screen.getByRole('button', { name: 'Estimation of WEB-12: 1d 2h' }))
      .toBeVisible();
    const put = calls.find((c) => c.method === 'PUT');
    expect(put?.body).toEqual({ value: 600 });
  });
});

describe('ISS-024, ISS-042, LNK-014: how rows show an issue', () => {
  it('mutes a resolved row, locks a restricted one and shows subtask progress', async () => {
    const resolved = issue({ id: 504, readable_id: 'WEB-15', resolved_at: '2026-10-22T10:00:00Z' });
    const parent = issue({
      id: 505,
      readable_id: 'WEB-16',
      summary: 'Checkout',
      audience: { users: [BORIS], groups: [] },
      subtasks: { total: 5, unresolved: 3 },
    });
    const { screen } = await show({ search: searchPage([resolved, parent]) });
    const rows = rowsOf(screen);
    await expect.element(rows.getByText('2/5')).toBeVisible();
    await expect.element(rows.getByText('2 of 5 subtasks resolved')).toBeInTheDocument();
    await expect.element(rows.getByText('(resolved)').first()).toBeInTheDocument();
    expect(rows.element().querySelector('s')?.textContent).toBe('WEB-15');
    await expect.element(rows.getByRole('img', { name: /Boris Lead/ })).toBeInTheDocument();
  });
});

describe('QRY-078: the column chooser', () => {
  async function openChooser(screen: Shown['screen']) {
    await screen.getByRole('button', { name: 'List settings' }).click();
    await screen.getByRole('menuitem', { name: 'Columns…' }).click();
    return screen.getByRole('dialog', { name: 'Columns' });
  }

  it('adds, removes and reorders columns and saves them', async () => {
    const { screen, calls } = await show({
      prefs: { list_columns: ['id', 'summary', 'field:11'] },
    });
    const dialog = await openChooser(screen);
    const shown = dialog.getByRole('list', { name: 'Shown columns' });
    await expect.element(shown.getByText('Priority')).toBeVisible();
    await dialog.getByRole('button', { name: 'Remove Priority' }).click();
    await dialog.getByRole('combobox', { name: 'Column to add' }).selectOptions('Votes');
    await dialog.getByRole('button', { name: 'Add' }).click();
    await dialog.getByRole('button', { name: 'Move Votes up' }).click();
    await expect.element(dialog.getByRole('button', { name: 'Remove Summary' })).toBeDisabled();
    await dialog.getByRole('button', { name: 'Save' }).click();
    expect(preferences.value('list_columns')).toEqual(['id', 'votes', 'summary']);
    await expect
      .poll(() => calls.find((c) => c.method === 'PATCH')?.body)
      .toEqual({ list_columns: ['id', 'votes', 'summary'] });
  });

  it('offers the fields of one project when narrowed, and resets to the default', async () => {
    const { screen, calls } = await show({
      routes: {
        'GET /projects': {
          body: { items: [{ id: WEB.id, key: WEB.key, name: WEB.name }], next_cursor: null },
        },
      },
      prefs: { list_columns: ['id', 'summary'] },
    });
    calls.length = 0;
    const dialog = await openChooser(screen);
    const add = dialog.getByRole('combobox', { name: 'Column to add' });
    await expect.element(add.getByRole('option', { name: 'Due date' })).toBeInTheDocument();
    await dialog.getByRole('combobox', { name: 'Fields of' }).selectOptions('Web');
    await expect.poll(() => calls.some((c) => c.query.get('project') === '1')).toBe(true);
    await dialog.getByRole('button', { name: 'Reset to default' }).click();
    expect(preferences.value('list_columns')).toEqual([]);
  });
});

describe('AUT-066: клонирование в списке', () => {
  /** Sees the project and creates issues in it. */
  const CREATOR: AccessMap = {
    global: [],
    projects: [{ project_id: 1, permissions: ['project.basic', 'issue.create'] }],
    capabilities: [],
  };

  function cloneRoutes(): Record<string, FakeRoute> {
    return {
      'GET /projects/1/rules': {
        body: {
          version: 2,
          items: [{ rule: 'clone_issue', name: 'Issue cloning', enabled: true, needs_setup: [] }],
        },
      },
      'POST /issues/501/clone': {
        status: 201,
        body: {
          issue: issue({
            id: 700,
            readable_id: 'WEB-15',
            number: 15,
            summary: 'Login button does nothing (copy)',
          }),
          messages: [{ rule: 'clone_issue', code: 'clone_created', params: { issue: 'WEB-15' } }],
        },
      },
      'POST /issues/clone-drafts': {
        body: {
          drafts: [issue({ id: 701, draft: true, readable_id: null, number: null })],
          messages: [{ rule: 'clone_issue', code: 'draft_saved', params: { issue: 'WEB-12' } }],
        },
      },
    };
  }

  it('the row clones when the project allows it: the table from the row’s menu, the card with touch-sized buttons', async () => {
    setAccessForTests(CREATOR);
    const { screen, calls } = await show({ routes: cloneRoutes() });
    calls.length = 0;

    if (isPhone()) {
      // The card's «Ещё» opens the actions; cloning works with touch (§3.13).
      await screen.getByRole('button', { name: 'More for WEB-12' }).click();
      const clone = page.getByRole('button', { name: 'Clone as draft' });
      const published = page.getByRole('button', { name: 'Clone', exact: true });
      await expect.element(clone).toBeVisible();
      // The card's own links and chips may stay small; the clone targets are touch-sized (§11).
      for (const button of [published.element(), clone.element()]) {
        const box = button.getBoundingClientRect();
        expect(box.width).toBeGreaterThanOrEqual(44);
        expect(box.height).toBeGreaterThanOrEqual(44);
      }
      await clone.click();

      await expect
        .poll(() => calls.find((call) => call.method === 'POST')?.body)
        .toEqual({ issues: [501] });
      expect(toasts.items.map((toast) => toast.message)).toEqual(['Draft copied from WEB-12']);
    } else {
      // Hovering the row asks for the project's toggle; the actions menu appears.
      await page.getByRole('link', { name: /WEB-12/ }).hover();
      const menu = page.getByRole('button', { name: 'Actions for WEB-12' });
      await expect.element(menu).toBeVisible();
      await menu.click();
      await page.getByRole('menuitem', { name: 'Clone' }).click();

      expect(calls.filter((call) => call.method === 'POST')).toEqual([
        {
          method: 'POST',
          path: '/issues/501/clone',
          query: new URLSearchParams(),
          body: undefined,
        },
      ]);
      expect(toasts.items.map((toast) => toast.message)).toEqual(['Copy created: WEB-15']);
    }
  });

  it('no clone actions when the project’s toggle is off', async () => {
    setAccessForTests(CREATOR);
    const { screen, calls } = await show({
      routes: {
        ...cloneRoutes(),
        'GET /projects/1/rules': {
          body: {
            version: 2,
            items: [
              { rule: 'clone_issue', name: 'Issue cloning', enabled: false, needs_setup: [] },
            ],
          },
        },
      },
    });
    if (isPhone()) {
      await screen.getByRole('button', { name: 'More for WEB-12' }).click();
    } else {
      await page.getByRole('link', { name: /WEB-12/ }).hover();
    }
    await vi.waitFor(() => {
      // The toggle's answer landed; the actions did not follow.
      expect(calls.some((call) => call.path === '/projects/1/rules')).toBe(true);
    });
    expect(page.getByRole('button', { name: 'Actions for WEB-12' }).query()).toBeNull();
    expect(page.getByRole('button', { name: 'Clone', exact: true }).query()).toBeNull();
  });
});
