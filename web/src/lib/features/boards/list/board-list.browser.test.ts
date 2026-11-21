// The boards entry, the list and the switcher on a desktop and on a phone (step 1.8a-4; BRD-001,
// BRD-002, PRJ-016).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type {
  AccessMap,
  BoardList as BoardPage,
  BoardSummary,
  ProjectRef,
  Session,
  UserRef,
} from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi, failure, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { boardEntry } from '../boards.ts';
import BoardList from './BoardList.svelte';
import BoardSwitcher from './BoardSwitcher.svelte';

const SESSION: Session = {
  user: {
    id: 1,
    username: 'anna',
    full_name: 'Anna Admin',
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

const ANNA: UserRef = {
  id: 1,
  full_name: 'Anna Admin',
  username: 'anna',
  avatar_color: 0,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

const BORIS: UserRef = { ...ANNA, id: 2, full_name: 'Boris Lead', username: 'boris' };

/** Reads issues in project 1. */
const READER: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic', 'issue.read'] }],
  capabilities: [],
};

/** Sees nothing it could put on a board. */
const NOBODY: AccessMap = { global: [], projects: [], capabilities: [] };

function project(id: number, name: string): ProjectRef {
  return {
    id,
    name,
    key: name.toUpperCase().slice(0, 3),
    archived: false,
    logo_color: 0,
    logo_url: null,
  };
}

function board(id: number, name: string, extra: Partial<BoardSummary> = {}): BoardSummary {
  return { id, name, favorite: false, owner: ANNA, projects: [project(1, 'Web')], ...extra };
}

const TEAM = board(1, 'Team', { favorite: true });
const RELEASE = board(2, 'Release', {
  owner: BORIS,
  projects: ['Web', 'Mobile', 'Docs', 'Site', 'Api'].map((name, index) => project(index + 1, name)),
});
const SUPPORT = board(3, 'Support');

function pageOf(items: BoardSummary[], next: string | null = null): BoardPage {
  return { items, next_cursor: next, last_visited: null };
}

function server(access: AccessMap, routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi(routes);
}

function rowNames(screen: Awaited<ReturnType<typeof render>>): string[] {
  return screen
    .getByRole('list', { name: 'Boards' })
    .getByRole('link')
    .elements()
    .map((link) => link.textContent.trim());
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-001: the entry and the list', () => {
  it('opens the board visited last, or the list when none was or the list is asked for', async () => {
    const calls = server(READER, {
      'GET /boards': { body: { ...pageOf([TEAM]), last_visited: 1 } },
    });
    expect(await boardEntry(false)).toEqual({ kind: 'board', id: 1 });
    expect((await boardEntry(true)).kind).toBe('list');
    expect(calls[0]?.query.get('limit')).toBe('50');

    server(READER, { 'GET /boards': { body: pageOf([]) } });
    expect((await boardEntry(false)).kind).toBe('list');
  });

  it('lists name, owner and projects with a link to each board', async () => {
    server(READER, {});
    const screen = await render(BoardList, { first: pageOf([TEAM, RELEASE, SUPPORT]) });

    await expect.element(screen.getByRole('heading', { level: 1, name: 'Boards' })).toBeVisible();
    expect(rowNames(screen)).toEqual(['Team', 'Release', 'Support']);
    const list = screen.getByRole('list', { name: 'Boards' });
    expect(list.getByRole('link', { name: 'Release' }).element().getAttribute('href')).toBe(
      '/boards/2',
    );
    await expect.element(list.getByText('Owner: Boris Lead')).toBeVisible();
    // Three projects by name, the rest as a count.
    await expect.element(list.getByText(/Web, Mobile, Docs\s+\+2/)).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Create board' })).toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('searches by name and loads the next page', async () => {
    const calls = server(READER, {
      'GET /boards': (request) => {
        if (request.query.get('cursor') === 'next') return { body: pageOf([SUPPORT]) };
        if (request.query.get('q') === 'rel') return { body: pageOf([RELEASE]) };
        return { body: pageOf([]) };
      },
    });
    const screen = await render(BoardList, { first: pageOf([TEAM, RELEASE], 'next') });

    await screen.getByRole('button', { name: 'Show more' }).click();
    await expect.poll(() => rowNames(screen)).toEqual(['Team', 'Release', 'Support']);
    await expect.element(screen.getByRole('button', { name: 'Show more' })).not.toBeInTheDocument();

    await screen.getByRole('searchbox', { name: 'Find boards' }).fill('rel');
    await expect.poll(() => rowNames(screen)).toEqual(['Release']);
    expect(calls.at(-1)?.query.get('q')).toBe('rel');

    await screen.getByRole('searchbox', { name: 'Find boards' }).fill('none');
    await expect.element(screen.getByText('No boards found')).toBeVisible();
  });

  it('shows the empty state; a person who may not create boards gets no button', async () => {
    server(READER, {});
    const screen = await render(BoardList, { first: pageOf([]) });
    await expect.element(screen.getByText('No boards yet')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Create board' }).elements()).toHaveLength(2);
    await expectAccessible(screen.container);
    await screen.unmount();

    server(NOBODY, {});
    const other = await render(BoardList, { first: pageOf([]) });
    await expect.element(other.getByText('Nobody has shared a board with you yet.')).toBeVisible();
    expect(other.getByRole('button', { name: 'Create board' }).elements()).toHaveLength(0);
  });

  it('PRJ-016: a project’s boards ask only for boards including it', async () => {
    const calls = server(READER, { 'GET /boards': { body: pageOf([]) } });
    const screen = await render(BoardList, {
      first: pageOf([]),
      project: { id: 1, name: 'Web', key: 'WEB', logo_color: 0 },
    });
    await expect.element(screen.getByText('No boards include this project yet')).toBeVisible();
    await screen.getByRole('searchbox', { name: 'Find boards' }).fill('x');
    await expect.poll(() => calls.at(-1)?.query.get('project')).toBe('1');
  });
});

describe('BRD-002: favorites', () => {
  it('stars a board through the profile and moves it up; a refusal puts it back', async () => {
    let refuse = false;
    const calls = server(READER, {
      'PATCH /boards/3/profile': () =>
        refuse
          ? failure(403, 'forbidden')
          : {
              body: { favorite: true, card_size: 1, collapsed_columns: [], collapsed_lanes: [] },
            },
    });
    const screen = await render(BoardList, { first: pageOf([TEAM, RELEASE, SUPPORT]) });

    const star = screen.getByRole('button', { name: 'Add “Support” to favorites' });
    await star.click();
    await expect
      .element(screen.getByRole('button', { name: 'Remove “Support” from favorites' }))
      .toHaveAttribute('aria-pressed', 'true');
    expect(rowNames(screen)).toEqual(['Support', 'Team', 'Release']);
    expect(calls.at(-1)).toMatchObject({ method: 'PATCH', body: { favorite: true } });

    refuse = true;
    await screen.getByRole('button', { name: 'Add “Release” to favorites' }).click();
    await expect.poll(() => calls.length).toBe(2);
    await expect
      .element(screen.getByRole('button', { name: 'Add “Release” to favorites' }))
      .toHaveAttribute('aria-pressed', 'false');
    expect(rowNames(screen)).toEqual(['Support', 'Team', 'Release']);
  });

  it('the switcher lists boards with owners, marks the current one and stars them', async () => {
    const calls = server(READER, {
      'GET /boards': (request) =>
        request.query.get('q') === 'sup'
          ? { body: pageOf([SUPPORT]) }
          : { body: pageOf([TEAM, RELEASE, SUPPORT]) },
      'PATCH /boards/2/profile': {
        body: { favorite: true, card_size: 1, collapsed_columns: [], collapsed_lanes: [] },
      },
    });
    const screen = await render(BoardSwitcher, { board: { id: 1, name: 'Team' } });

    await screen.getByRole('button', { name: 'Board “Team”: switch board' }).click();
    const list = screen.getByRole('list', { name: 'Boards' });
    await expect
      .element(list.getByRole('link', { name: /Team/ }))
      .toHaveAttribute('aria-current', 'page');
    await expect.element(list.getByText('Boris Lead')).toBeVisible();
    await expect
      .element(screen.getByRole('link', { name: 'All boards' }))
      .toHaveAttribute('href', '/boards?list');

    await screen.getByRole('button', { name: 'Add “Release” to favorites' }).click();
    await expect
      .element(screen.getByRole('button', { name: 'Remove “Release” from favorites' }))
      .toHaveAttribute('aria-pressed', 'true');
    expect(calls.at(-1)).toMatchObject({ method: 'PATCH', path: '/boards/2/profile' });

    await screen.getByRole('searchbox', { name: 'Filter boards' }).fill('sup');
    await expect.poll(() => list.getByRole('link').elements().length).toBe(1);
    await expectAccessible(document.body);
  });
});
