// The issue page in read form on a desktop and on a phone (step 1.7a; ISS-001, ISS-003, ISS-016,
// ISS-020, FLD-057, FLD-062, UI-005).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Attachment, Comment, Issue } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { preferences, setAccessForTests, setPreferencesForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi } from '../../../../test/fake-api.ts';
import { field, issue, issuePage, BORIS } from '../test-data.ts';
import { forgetIssuePages, openIssue } from '../issues.ts';
import IssuePage from './IssuePage.svelte';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

/** Sees project 1. */
const READER: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic'] }],
  capabilities: [],
};

/** Configures project 1. */
const LEAD: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic', 'project.update'] }],
  capabilities: [],
};

function show(shown: Issue, access: AccessMap = READER, requested?: string) {
  setAccessForTests(access);
  setPreferencesForTests({});
  return render(IssuePage, { issue: shown, requested });
}

afterEach(() => {
  vi.unstubAllGlobals();
  navigation.goto.mockClear();
  store.entities.table('issue').clear();
  store.entities.table('comment').clear();
  store.entities.table('attachment').clear();
  forgetIssuePages();
  setAccessForTests(null);
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
});

describe('IssuePage', () => {
  it('ISS-001, ISS-016: header, metadata strip, description and text fields', async () => {
    const screen = await show(issue());

    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Login button does nothing' }))
      .toBeVisible();
    expect(screen.getByRole('heading', { level: 1 }).elements()).toHaveLength(1);
    await expect.element(screen.getByText('WEB-12', { exact: true })).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Copy ID' })).toBeVisible();

    const details = screen.getByRole('group', { name: 'About the issue' });
    await expect.element(details.getByText('Anna Admin')).toBeVisible();
    await expect.element(details.getByText('Boris Lead')).toBeVisible();
    await expect
      .element(details.getByText('Visible to everyone who sees the project'))
      .toBeVisible();
    await expect.element(details.getByText('3 votes')).toBeInTheDocument();
    // NTF-015: the count next to the star is the button that opens the watcher list.
    await expect.element(details.getByRole('button', { name: '2 watchers' })).toBeInTheDocument();
    await expect
      .element(details.getByRole('button', { name: /^Watch/, pressed: true }))
      .toBeVisible();
    await expect
      .element(details.getByRole('button', { name: /^Vote/, pressed: false }))
      .toBeVisible();

    // The description's headings sit under the page's.
    await expect.element(screen.getByRole('heading', { level: 3, name: 'Cause' })).toBeVisible();
    // Text fields are blocks under the description, not rows of the panel.
    await expect
      .element(screen.getByRole('heading', { level: 2, name: 'Steps to reproduce' }))
      .toBeVisible();
    await expect.element(screen.getByText('Open the page')).toBeVisible();
    const panel = screen.getByRole('complementary', { name: 'Fields' });
    expect(panel.getByText('Steps to reproduce').query()).toBeNull();

    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ISS-016, ISS-041: a restricted issue names its audience with the reporter', async () => {
    const screen = await show(
      issue({
        audience: {
          users: [BORIS],
          groups: [{ id: 7, name: 'Developers', builtin: null, team_of: null }],
        },
      }),
    );

    const details = screen.getByRole('group', { name: 'About the issue' });
    await expect.element(details.getByText('Restricted', { exact: true })).toBeVisible();
    await expect
      .element(details.getByText('Visible to Developers, Boris Lead and the reporter'))
      .toBeVisible();
  });

  it('ISS-020: the panel shows the project first, then the fields in order', async () => {
    const screen = await show(issue(), LEAD);
    const panel = screen.getByRole('complementary', { name: 'Fields' });

    const names = panel
      .getByRole('term')
      .elements()
      .map((term) => term.textContent.trim());
    expect(names).toEqual(['Project', 'Priority', 'Assignee', 'Estimation', 'Due date']);
    expect(panel.getByRole('link', { name: 'Web' }).element().getAttribute('href')).toMatch(
      /\/projects\/WEB$/,
    );
    // PRJ-016: people who configure the project reach its settings from here.
    expect(
      panel.getByRole('link', { name: 'Settings of Web' }).element().getAttribute('href'),
    ).toMatch(/\/projects\/WEB\/settings\/general$/);
    await expect.element(panel.getByText('Major')).toBeVisible();
    await expect.element(panel.getByText('Boris Lead')).toBeVisible();
    await expect.element(panel.getByText('1h 30m')).toBeVisible();
    expect(panel.element().querySelector('time')?.getAttribute('datetime')).toBe('2026-11-02');
  });

  it('ISS-020: a reader has no link to the settings; empty fields show their label', async () => {
    const screen = await show(
      issue({ fields: [field(12, 'Assignee', 'user', { empty_label: 'Unassigned' })] }),
    );
    const panel = screen.getByRole('complementary', { name: 'Fields' });

    await expect.element(panel.getByText('Unassigned')).toBeVisible();
    expect(panel.getByRole('link', { name: 'Settings of Web' }).query()).toBeNull();
  });

  it('FLD-057, FLD-062: empty required fields say so; private fields carry a lock', async () => {
    const screen = await show(
      issue({
        fields: [
          field(21, 'Severity', 'enum', { required: true }),
          field(22, 'Cost', 'integer', { private: true, integer: 1200 }),
        ],
      }),
    );
    const panel = screen.getByRole('complementary', { name: 'Fields' });

    await expect.element(panel.getByText('Required field')).toBeVisible();
    await expect.element(panel.getByRole('img', { name: 'Hidden field' })).toBeVisible();
    await expect.element(panel.getByText('1,200')).toBeVisible();
    expect(panel.getByRole('img', { name: 'Hidden field' }).elements()).toHaveLength(1);
  });

  it('ISS-020: side, width and collapsed state come from personal settings (desktop)', async () => {
    if (isPhone()) return;
    setAccessForTests(READER);
    setPreferencesForTests({ issue_panel_side: 'start', issue_panel_width: 300 });
    const screen = await render(IssuePage, { issue: issue() });
    const panel = screen.getByRole('complementary', { name: 'Fields' });
    const content = screen.getByRole('region', { name: 'Description' });

    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
    expect(panel.element().getBoundingClientRect().width).toBeCloseTo(30 * rem, 0);
    expect(panel.element().getBoundingClientRect().right).toBeLessThan(
      content.element().getBoundingClientRect().left,
    );

    // The handle moves by 1 rem a key; the panel is at the start, so right widens it.
    const handle = screen.getByRole('separator', { name: 'Width of the field panel' });
    await expect.element(handle).toHaveAttribute('aria-valuenow', '300');
    (handle.element() as HTMLElement).focus();
    await userEvent.keyboard('{ArrowRight}');
    expect(preferences.value('issue_panel_width')).toBe(310);
    await userEvent.keyboard('{End}');
    expect(preferences.value('issue_panel_width')).toBe(400);

    await screen.getByRole('button', { name: 'Collapse the field panel' }).click();
    expect(preferences.value('issue_panel_collapsed')).toBe(true);
    expect(panel.getByRole('term').query()).toBeNull();
    expect(screen.getByRole('separator').query()).toBeNull();
    await screen.getByRole('button', { name: 'Expand the field panel' }).click();
    expect(preferences.chosen('issue_panel_collapsed')).toBeUndefined();
    await expect.element(panel.getByText('Priority')).toBeVisible();
  });

  it('ISS-020: on phones the fields are a section above the content, never collapsed', async () => {
    if (!isPhone()) return;
    setAccessForTests(READER);
    setPreferencesForTests({ issue_panel_collapsed: true });
    const screen = await render(IssuePage, { issue: issue() });
    const panel = screen.getByRole('complementary', { name: 'Fields' });

    await expect.element(panel.getByRole('heading', { level: 2, name: 'Fields' })).toBeVisible();
    await expect.element(panel.getByText('Priority')).toBeVisible();
    expect(panel.element().getBoundingClientRect().bottom).toBeLessThanOrEqual(
      screen.getByRole('region', { name: 'Description' }).element().getBoundingClientRect().top,
    );
    expect(screen.getByRole('button', { name: /field panel/ }).query()).toBeNull();
    expect(screen.getByRole('separator').query()).toBeNull();
  });

  it('UI-005: breadcrumbs lead to the project and its issues; phones get a back link', async () => {
    const screen = await show(issue());
    const crumbs = screen.getByRole('navigation', { name: 'Breadcrumbs' });

    if (isPhone()) {
      const back = crumbs.getByRole('link', { name: 'Issues WEB' });
      await expect.element(back).toBeVisible();
      expect(back.element().getAttribute('href')).toMatch(/\/issues\?project=WEB$/);
      expect(crumbs.getByRole('link', { name: 'Web' }).query()).toBeNull();
    } else {
      expect(
        crumbs
          .getByRole('link')
          .elements()
          .map((link) => [link.textContent.trim(), link.getAttribute('href')]),
      ).toEqual([
        ['Web', expect.stringMatching(/\/projects\/WEB$/)],
        ['Issues', expect.stringMatching(/\/issues\?project=WEB$/)],
      ]);
    }
  });

  it('ISS-003: opened by a former ID, the page puts the current one in the address', async () => {
    await show(issue(), READER, 'SITE-12');

    await vi.waitFor(() => {
      expect(navigation.goto).toHaveBeenCalledWith(
        expect.stringMatching(/\/issues\/WEB-12$/),
        expect.objectContaining({ replace: true }),
      );
    });
  });

  it('ISS-003: opened by its current ID, the address stays', async () => {
    await show(issue(), READER, 'WEB-12');
    await new Promise((settle) => setTimeout(settle, 50));
    expect(navigation.goto).not.toHaveBeenCalled();
  });
});

describe('openIssue', () => {
  it('ISS-002, ISS-003: finds an issue by any ID, tells a moved one and remembers it', async () => {
    setPreferencesForTests({ recent_issues: [7, 501, 8] });
    const calls = fakeApi({
      'GET /issues/SITE-12/page': { body: issuePage() },
      'GET /issues/WEB-12/page': { body: issuePage() },
    });

    const moved = await openIssue('SITE-12');
    expect(moved).toMatchObject({ kind: 'moved', readableId: 'WEB-12' });
    const found = await openIssue('WEB-12');
    expect(found.kind).toBe('found');
    expect(found.kind === 'found' && found.issue.current?.summary).toBe(
      'Login button does nothing',
    );
    expect(calls.map((call) => call.path)).toEqual(['/issues/SITE-12/page', '/issues/WEB-12/page']);
    // LNK-005: the opened issue comes first in the recent ones.
    expect(preferences.value('recent_issues')).toEqual([501, 7, 8]);
  });

  it('ISS-001: one request opens the page; its comments and files go into the store', async () => {
    setPreferencesForTests({ activity_changes: false });
    const comment = { id: 31, version: 2, text: 'Seen it' } as unknown as Comment;
    const file = { id: 41, version: 1, name: 'shot.png' } as unknown as Attachment;
    const calls = fakeApi({
      'GET /issues/WEB-12/page': {
        body: issuePage(issue(), {
          activity: {
            items: [{ type: 'comment', id: 90, at: '2026-10-24T10:00:00Z', comment }],
            older: null,
            newer: null,
          },
          attachments: { attachments: [file], next: null },
        }),
      },
    });

    const opened = await openIssue('WEB-12');
    // A preload on hover and the navigation after it share the answer.
    await openIssue('WEB-12');
    expect(calls).toHaveLength(1);
    const query = calls[0]?.query;
    expect([query?.get('limit'), query?.get('comments'), query?.get('changes')]).toEqual([
      '50',
      'true',
      'false',
    ]);
    expect(opened.kind === 'found' && opened.page.activity.items).toHaveLength(1);
    expect(store.entities.table('comment').get(31)?.text).toBe('Seen it');
    expect(store.entities.table('attachment').get(41)?.name).toBe('shot.png');
  });

  it('ISS-002: a missing or hidden issue is missing', async () => {
    fakeApi({});
    expect(await openIssue('WEB-404')).toEqual({ kind: 'missing' });
  });

  it('live refetches of many issues cost one request', async () => {
    const calls = fakeApi({
      'POST /issues/batch': ({ body }) => ({
        body: { items: (body as { ids: number[] }).ids.map((id) => issue({ id, version: 5 })) },
      }),
    });
    store.entities.table('issue').upsertMany([1, 2, 3].map((id) => issue({ id })));

    await store.refetch('issue', [1, 2, 3]);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.body).toEqual({ ids: [1, 2, 3] });
    expect(store.entities.table('issue').get(2)?.version).toBe(5);
  });
});
