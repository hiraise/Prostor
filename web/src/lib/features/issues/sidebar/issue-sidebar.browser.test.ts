// The issue list's sidebar on a desktop and on a phone (step 1.7c-3): its groups and counts, folding,
// modes and width, the phone drawer (QRY-082), drafts (ISS-011), editing and deleting tags and
// searches (ISS-036, QRY-064) and sharing them (QRY-065, ISS-037).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  Group,
  Preferences,
  Project,
  SavedSearch,
  Session,
  SharingAbilities,
  Tag,
} from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { preferences, setPreferencesForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import type { ListContext, ResolvedContext } from '../list/list-state.ts';
import { newIssueDialog } from '../new/open.svelte.ts';
import { ANNA, BORIS, issue } from '../test-data.ts';
import IssueSidebar from './IssueSidebar.svelte';

const SESSION: Session = {
  user: {
    id: ANNA.id,
    username: ANNA.username,
    full_name: ANNA.full_name,
    email: 'anna@example.com',
    email_verified: true,
    avatar_color: 0,
    avatar_url: null,
    version: 1,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

const ALL: SharingAbilities = {
  apply: true,
  edit: true,
  delete: true,
  share: true,
  transfer: false,
};
const USE_ONLY: SharingAbilities = {
  apply: true,
  edit: false,
  delete: false,
  share: false,
  transfer: false,
};
const NOBODY = { users: [], groups: [] };

function project(id: number, key: string, name: string, favorite = false): Project {
  return { id, key, name, favorite } as unknown as Project;
}

function tag(id: number, name: string, rest: Partial<Tag> = {}): Tag {
  return {
    id,
    name,
    color: 3,
    owner: ANNA,
    sharing: { view: NOBODY, use: NOBODY, edit: NOBODY },
    abilities: ALL,
    version: 4,
    created_at: '2026-10-20T09:00:00Z',
    ...rest,
  };
}

function search(id: number, name: string, rest: Partial<SavedSearch> = {}): SavedSearch {
  return {
    id,
    name,
    owner: ANNA,
    query: 'assignee: me',
    problems: [],
    builtin: null,
    project: null,
    sharing: { view: NOBODY, edit: NOBODY },
    abilities: { ...ALL, apply: false },
    version: 2,
    created_at: '2026-10-20T09:00:00Z',
    updated_at: '2026-10-20T09:00:00Z',
    ...rest,
  };
}

const MOBILE = project(1, 'MOB', 'Mobile');
const WEBSITE = project(2, 'WEB', 'Web', true);
const URGENT = tag(5, 'urgent');
const THEIRS = tag(6, 'later', { owner: BORIS, abilities: USE_ONLY, sharing: null, color: null });
const MINE = search(7, 'My open');
const BUILTIN = search(8, 'Assigned to me', { builtin: 'assigned_to_me' });
const SHARED = search(9, 'Team bugs', {
  owner: BORIS,
  abilities: { ...USE_ONLY, apply: false },
  sharing: null,
});
const DRAFT = issue({ id: 900, readable_id: '', summary: 'Half written' });
const EMPTY_DRAFT = issue({ id: 901, readable_id: '', summary: '' });

const TEAM = { id: 30, name: 'Designers', builtin: null, team_of: null } as unknown as Group;

interface Shown {
  screen: Awaited<ReturnType<typeof render>>;
  calls: FakeRequest[];
  onchoose: ReturnType<typeof vi.fn<(context: ListContext) => void>>;
}

async function show(
  options: {
    context?: ResolvedContext;
    drafts?: (typeof DRAFT)[];
    prefs?: Preferences;
    hideResolved?: boolean;
    drawer?: boolean;
    routes?: Record<string, FakeRoute>;
  } = {},
): Promise<Shown> {
  const calls = fakeApi({
    'PATCH /users/me/preferences': { body: {} },
    'GET /issues/drafts': { body: { items: options.drafts ?? [], next_cursor: null } },
    'GET /projects': { body: { items: [MOBILE, WEBSITE], next_cursor: null } },
    'GET /tags': { body: { items: [THEIRS, URGENT], next_cursor: null } },
    'GET /saved-searches': { body: { items: [SHARED, MINE, BUILTIN], next_cursor: null } },
    'POST /issues/counts': (request) => ({
      body: {
        items: (request.body as { queries: unknown[] }).queries.map((_, i) => ({
          all: 10 + i,
          unresolved: i,
          problems: [],
        })),
      },
    }),
    ...options.routes,
  });
  setSessionForTests(SESSION);
  setPreferencesForTests(options.prefs ?? {});
  const onchoose = vi.fn<(context: ListContext) => void>();
  const screen = await render(IssueSidebar, {
    props: {
      context: options.context ?? { kind: 'all' },
      hideResolved: options.hideResolved ?? false,
      drawer: options.drawer ?? isPhone(),
      onchoose,
    },
  });
  return { screen, calls, onchoose };
}

function lists(screen: Shown['screen']) {
  return screen.getByRole('navigation', { name: 'Lists' });
}

function counts(calls: FakeRequest[]): FakeRequest[] {
  return calls.filter((c) => c.method === 'POST' && c.path === '/issues/counts');
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  store.entities.table('issue').clear();
  setPreferencesForTests(null);
  setSessionForTests(null);
  newIssueDialog.open = false;
  setLocale(BASE_LOCALE);
});

describe('QRY-082: the sidebar', () => {
  it('lists a search saved from the list at once, reloading only once', async () => {
    const fresh = search(42, 'Fresh widgets');
    let saved = false;
    const { screen, calls } = await show({
      routes: {
        'GET /saved-searches': () => ({
          body: { items: saved ? [MINE, fresh] : [MINE], next_cursor: null },
        }),
      },
    });
    const nav = lists(screen);
    await expect.element(nav.getByRole('link', { name: /^My open/ })).toBeVisible();
    saved = true;
    await screen.rerender({
      context: {
        kind: 'search',
        id: 42,
        name: 'Fresh widgets',
        query: 'widget',
        projectKey: null,
        editable: true,
        mine: true,
        version: 1,
      },
    });
    await expect.element(nav.getByRole('link', { name: /^Fresh widgets/ })).toBeVisible();
    expect(calls.filter((c) => c.path === '/saved-searches')).toHaveLength(2);
  });

  it('shows drafts, projects, tags and searches in order, counted in one request', async () => {
    const { screen, calls } = await show({ drafts: [DRAFT] });
    const nav = lists(screen);
    await expect.element(nav.getByRole('link', { name: /^Web/ })).toBeVisible();
    const headings = nav
      .getByRole('heading', { level: 2 })
      .elements()
      .map((e) => e.textContent.trim());
    expect(headings).toEqual(['Drafts', 'Projects', 'Tags', 'Saved searches']);
    // Favorites first; own tags first; predefined searches, then own, then shared.
    const names = nav
      .getByRole('link')
      .elements()
      .map((e) => e.querySelector('.name')?.textContent);
    expect(names).toEqual([
      'Web',
      'Mobile',
      'urgent',
      'later',
      'Assigned to me',
      'My open',
      'Team bugs',
    ]);
    expect(counts(calls)).toHaveLength(1);
    expect(counts(calls)[0]?.body).toEqual({
      queries: [
        { project: 2 },
        { project: 1 },
        { tag: 5 },
        { tag: 6 },
        { saved_search: 8 },
        { saved_search: 7 },
        { saved_search: 9 },
      ],
    });
    await expect.element(nav.getByRole('link', { name: 'Web, 10 issues' })).toBeVisible();
    await expect
      .element(nav.getByRole('link', { name: 'urgent, 12 issues' }))
      .toHaveAttribute('href', '/issues?tag=5');
    await expectAccessible(screen.container.ownerDocument.body);
  });

  it('counts unresolved issues while resolved ones are hidden, and marks the current list', async () => {
    const { screen } = await show({
      hideResolved: true,
      context: { kind: 'project', id: 1, key: 'MOB', name: 'Mobile' },
    });
    const mobile = lists(screen).getByRole('link', { name: 'Mobile, 1 issue' });
    await expect.element(mobile).toHaveAttribute('aria-current', 'page');
    await expect.element(lists(screen).getByRole('link', { name: 'Web, 0 issues' })).toBeVisible();
  });

  it('folds a group and keeps it folded', async () => {
    const { screen } = await show({ prefs: { list_sidebar_folded: ['projects'] } });
    const nav = lists(screen);
    const projects = nav.getByRole('button', { name: 'Projects' });
    await expect.element(projects).toHaveAttribute('aria-expanded', 'false');
    await expect.element(nav.getByRole('link', { name: /^urgent/ })).toBeVisible();
    expect(nav.getByRole('link', { name: /^Web/ }).query()).toBeNull();
    await nav.getByRole('button', { name: 'Tags' }).click();
    expect(preferences.value('list_sidebar_folded')).toEqual(['projects', 'tags']);
    await projects.click();
    expect(preferences.value('list_sidebar_folded')).toEqual(['tags']);
    await expect.element(nav.getByRole('link', { name: /^Web/ })).toBeVisible();
  });

  it.skipIf(isPhone())('is hidden, shown on focus or always, with the width chosen', async () => {
    const { screen } = await show({ prefs: { list_sidebar: 'closed' } });
    expect(screen.getByRole('navigation', { name: 'Lists' }).query()).toBeNull();

    preferences.set({ list_sidebar: 'auto' });
    const peek = screen.getByRole('button', { name: 'Lists' });
    await expect.element(peek).toHaveAttribute('aria-expanded', 'false');
    const visibility = (): string | undefined => {
      const nav = screen.container.querySelector('nav');
      return nav ? getComputedStyle(nav).visibility : undefined;
    };
    expect(visibility()).toBe('hidden');
    await peek.click();
    await expect.element(peek).toHaveAttribute('aria-expanded', 'true');
    await expect.poll(visibility).toBe('visible');

    preferences.set({ list_sidebar: 'open', list_sidebar_width: 300 });
    const handle = screen.getByRole('separator', { name: 'Width of the lists' });
    await expect.element(handle).toHaveAttribute('aria-valuenow', '300');
    await handle.click();
    await userEvent.keyboard('{End}');
    expect(preferences.value('list_sidebar_width')).toBe(400);
  });

  it.runIf(isPhone())('is a drawer on phones', async () => {
    const { screen } = await show({ drawer: true });
    const drawer = screen.getByRole('dialog', { name: 'Lists' });
    await expect.element(drawer).toBeVisible();
    // Stay on the test page.
    drawer.element().addEventListener('click', (event) => {
      event.preventDefault();
    });
    await drawer.getByRole('link', { name: /^Mobile/ }).click();
    await expect.element(drawer).not.toBeInTheDocument();
  });
});

describe('ISS-011: drafts in the sidebar', () => {
  it('has no drafts group without drafts', async () => {
    const { screen } = await show();
    await expect.element(lists(screen).getByRole('button', { name: 'Projects' })).toBeVisible();
    expect(lists(screen).getByRole('button', { name: 'Drafts' }).query()).toBeNull();
  });

  it('continues a draft, deletes one and deletes all', async () => {
    let drafts = [DRAFT, EMPTY_DRAFT];
    const { screen, calls } = await show({
      routes: {
        // The list reads the drafts again when the form closes.
        'GET /issues/drafts': () => ({ body: { items: drafts, next_cursor: null } }),
        'DELETE /issues/901': () => {
          drafts = drafts.filter((d) => d.id !== 901);
          return { status: 204 };
        },
        'DELETE /issues/drafts': () => {
          drafts = [];
          return { status: 204 };
        },
      },
    });
    const nav = lists(screen);
    await nav.getByRole('button', { name: 'Delete the draft “Untitled”' }).click();
    await expect
      .poll(() => calls.some((c) => c.method === 'DELETE' && c.path === '/issues/901'))
      .toBe(true);
    await expect.element(nav.getByRole('button', { name: /Untitled/ })).not.toBeInTheDocument();

    if (!isPhone()) {
      await nav.getByRole('button', { name: 'Continue the draft “Half written”' }).click();
      expect(newIssueDialog.open).toBe(true);
      expect(newIssueDialog.target).toEqual({ draft: 900 });
      newIssueDialog.open = false;
    }

    await nav.getByRole('button', { name: 'Delete all drafts' }).click();
    const confirm = screen.getByRole('dialog', { name: 'Delete all drafts?' });
    await expect
      .element(confirm.getByText('The draft will be deleted.', { exact: false }))
      .toBeVisible();
    await confirm.getByRole('button', { name: 'Delete all drafts' }).click();
    await expect
      .poll(() => calls.some((c) => c.method === 'DELETE' && c.path === '/issues/drafts'))
      .toBe(true);
    await expect.element(nav.getByRole('button', { name: 'Drafts' })).not.toBeInTheDocument();
  });
});

describe('ISS-036: renaming, recoloring and deleting tags and searches', () => {
  it('renames and recolors an own tag', async () => {
    const { screen, calls } = await show({
      routes: { 'PATCH /tags/5': { body: tag(5, 'blocker', { color: 1, version: 5 }) } },
    });
    await lists(screen).getByRole('button', { name: 'Actions: urgent' }).click();
    await screen.getByRole('menuitem', { name: 'Edit' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Edit tag' });
    await dialog.getByRole('textbox', { name: 'Name' }).fill('blocker');
    await dialog.getByRole('radio', { name: 'Red' }).click();
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    const patch = calls.find((c) => c.method === 'PATCH' && c.path === '/tags/5');
    expect(patch?.body).toEqual({ name: 'blocker', color: 1, version: 4 });
  });

  it('deletes an own tag after saying how many issues carry it', async () => {
    const { screen, calls, onchoose } = await show({
      context: { kind: 'tag', id: 5, name: 'urgent' },
      routes: {
        'GET /tags/5/usage': { body: { issues: 3 } },
        'DELETE /tags/5': { status: 204 },
      },
    });
    await lists(screen).getByRole('button', { name: 'Actions: urgent' }).click();
    await screen.getByRole('menuitem', { name: 'Delete' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Delete the tag “urgent”?' });
    await expect.element(dialog.getByText(/taken off 3 issues/)).toBeVisible();
    await dialog.getByRole('button', { name: 'Delete' }).click();
    await expect
      .poll(() => calls.some((c) => c.method === 'DELETE' && c.path === '/tags/5'))
      .toBe(true);
    // The list showed this tag; it goes to all issues.
    expect(onchoose).toHaveBeenCalledWith({ kind: 'all' });
  });

  it('offers no actions on a tag shared only for use, and no delete to an editor', async () => {
    const editor = search(10, 'Edited by me', {
      owner: BORIS,
      abilities: { ...USE_ONLY, apply: false, edit: true },
      sharing: { view: NOBODY, edit: NOBODY },
    });
    const { screen } = await show({
      routes: {
        'GET /saved-searches': { body: { items: [editor], next_cursor: null } },
      },
    });
    const nav = lists(screen);
    await expect.element(nav.getByRole('link', { name: /^later/ })).toBeVisible();
    expect(nav.getByRole('button', { name: 'Actions: later' }).query()).toBeNull();
    await nav.getByRole('button', { name: 'Actions: Edited by me' }).click();
    await expect.element(screen.getByRole('menuitem', { name: 'Edit' })).toBeVisible();
    expect(screen.getByRole('menuitem', { name: 'Delete' }).query()).toBeNull();
    expect(screen.getByRole('menuitem', { name: 'Sharing…' }).query()).toBeNull();
  });

  it('changes the query of a saved search', async () => {
    const { screen, calls } = await show({
      routes: { 'PATCH /saved-searches/7': { body: search(7, 'My open', { version: 3 }) } },
    });
    await lists(screen).getByRole('button', { name: 'Actions: My open' }).click();
    await screen.getByRole('menuitem', { name: 'Edit' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Edit saved search' });
    await dialog.getByRole('textbox', { name: 'Query' }).fill('assignee: me #Unresolved');
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    const patch = calls.find((c) => c.method === 'PATCH' && c.path === '/saved-searches/7');
    expect(patch?.body).toEqual({ query: 'assignee: me #Unresolved', version: 2 });
  });
});

describe('QRY-065, ISS-037: sharing through the dialog', () => {
  it('shares a search for viewing with a person and a group', async () => {
    const { screen, calls } = await show({
      routes: {
        'GET /users': {
          body: { items: [{ ...BORIS, email: null, groups: [], projects: [] }], next_cursor: null },
        },
        'GET /groups': { body: { items: [TEAM], next_cursor: null } },
        'PATCH /saved-searches/7': { body: search(7, 'My open', { version: 3 }) },
      },
    });
    await lists(screen).getByRole('button', { name: 'Actions: My open' }).click();
    await screen.getByRole('menuitem', { name: 'Sharing…' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Sharing: My open' });
    const view = dialog.getByRole('group', { name: 'Who sees and runs it' });
    await expect.element(view.getByRole('radio', { name: 'Only me' })).toBeChecked();
    await view.getByRole('radio', { name: 'Chosen people and groups' }).click();
    await dialog.getByRole('checkbox', { name: /Boris Lead/ }).click();
    await dialog.getByRole('checkbox', { name: /Designers/ }).click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    const patch = calls.find((c) => c.method === 'PATCH' && c.path === '/saved-searches/7');
    expect(patch?.body).toEqual({
      sharing: { view: { users: [2], groups: [30] }, edit: { users: [], groups: [] } },
      version: 2,
    });
  });

  it('asks for someone when the chosen audience is empty, and keeps a tag to its owner', async () => {
    const { screen, calls } = await show({
      routes: {
        'GET /users': { body: { items: [], next_cursor: null } },
        'GET /groups': { status: 403, body: { code: 'forbidden', message: 'no' } },
        'PATCH /tags/5': { body: tag(5, 'urgent', { version: 5 }) },
      },
    });
    await lists(screen).getByRole('button', { name: 'Actions: urgent' }).click();
    await screen.getByRole('menuitem', { name: 'Sharing…' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Sharing: urgent' });
    const use = dialog.getByRole('group', { name: 'Who puts it on issues and takes it off' });
    await use.getByRole('radio', { name: 'Chosen people and groups' }).click();
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect.element(dialog.getByText('Choose someone, or keep it to yourself.')).toBeVisible();
    expect(calls.some((c) => c.method === 'PATCH')).toBe(false);
    await use.getByRole('radio', { name: 'Only me' }).click();
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    const patch = calls.find((c) => c.method === 'PATCH' && c.path === '/tags/5');
    expect(patch?.body).toEqual({
      sharing: { view: NOBODY, use: NOBODY, edit: NOBODY },
      version: 4,
    });
  });
});
