// The activity stream of the issue page (step 1.7b-2): items, change records, diffs, filters,
// order, paging, deep links and grouping.
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { ActivityPage } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { preferences, setPreferencesForTests } from '#lib/session/index.ts';
import type { Preferences } from '#lib/api/schema.gen.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRoute } from '../../../../test/fake-api.ts';
import {
  ASSIGNEE,
  BORIS,
  PRIORITY,
  change,
  changeItem,
  comment,
  commentItem,
  issue,
  issuePage,
  values,
} from '../test-data.ts';
import { forgetIssuePages, openIssue } from '../issues.ts';
import ActivityStream from './ActivityStream.svelte';
import { ActivityStream as Stream } from './activity.svelte.ts';

const box = createRawSnippet(() => ({
  render: () => '<form aria-label="Comment box"><p>Box</p></form>',
}));

const MAJOR = { id: 101, name: 'Major', color: 3, deleted: false };
const CRITICAL = { id: 102, name: 'Critical', color: 1, deleted: false };

/** Two comments and a field change between them, as the server sends them: newest first. */
function page(rest: Partial<ActivityPage> = {}): ActivityPage {
  return {
    items: [
      commentItem(30, comment(3, 'Second comment', { created_at: '2026-10-22T10:00:00Z' })),
      changeItem(
        change(20, {
          field: PRIORITY.field,
          removed: values({ values: [MAJOR] }),
          added: values({ values: [CRITICAL] }),
        }),
      ),
      commentItem(10, comment(1, 'First comment')),
    ],
    older: null,
    newer: null,
    ...rest,
  };
}

async function show(
  activity: ActivityPage,
  options: { prefs?: Preferences; routes?: Record<string, FakeRoute>; focus?: number } = {},
) {
  const calls = fakeApi({ 'PATCH /users/me/preferences': { body: {} }, ...options.routes });
  setPreferencesForTests(options.prefs ?? {});
  const stream = new Stream(501, activity, options.focus);
  const screen = await render(ActivityStream, { issue: issue(), stream, box });
  return { screen, stream, calls };
}

function texts(element: Element): string[] {
  return [...element.querySelectorAll('ol > li')].map((item) => item.textContent.trim());
}

afterEach(() => {
  vi.unstubAllGlobals();
  store.entities.table('comment').clear();
  store.entities.table('issue').clear();
  forgetIssuePages();
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
});

describe('HIST-001: the activity stream', () => {
  it('shows comments and changes in time order with their authors', async () => {
    // Oldest first is the default: the server's newest-first page is turned around.
    const { screen } = await show(page({ items: [...page().items].reverse() }));

    await expect.element(screen.getByRole('region', { name: 'Activity' })).toBeVisible();
    await expect.element(screen.getByText('First comment')).toBeVisible();
    const rows = texts(screen.container);
    expect(rows).toHaveLength(3);
    expect(rows[0]).toContain('First comment');
    expect(rows[1]).toContain('Priority');
    expect(rows[2]).toContain('Second comment');
    await expect
      .element(screen.getByRole('article', { name: 'Comment by Anna Admin' }).first())
      .toBeVisible();
    await expect.element(screen.getByText('Boris Lead')).toBeVisible();
    await expectAccessible(screen.container);
  });

  it('says when there is nothing yet', async () => {
    const { screen } = await show({ items: [], older: null, newer: null });
    await expect.element(screen.getByText('No comments or changes yet.')).toBeVisible();
  });
});

describe('HIST-004: change records', () => {
  it('strikes removed values and marks added ones', async () => {
    const { screen } = await show(page());

    const removed = screen.container.querySelector('del');
    const added = screen.container.querySelector('ins');
    expect(removed?.textContent).toContain('Major');
    expect(removed?.textContent).toContain('removed:');
    expect(added?.textContent).toContain('Critical');
    expect(added?.textContent).toContain('added:');
  });

  it("shows the field's empty label when a value is cleared", async () => {
    const { screen } = await show({
      items: [
        changeItem(
          change(5, {
            field: ASSIGNEE.field,
            removed: values({ users: [BORIS] }),
            added: values(),
          }),
        ),
      ],
      older: null,
      newer: null,
    });

    await expect.element(screen.getByText('Unassigned')).toBeVisible();
    expect(screen.container.querySelector('del')?.textContent).toContain('Boris Lead');
    expect(screen.container.querySelector('ins')).toBeNull();
  });

  it('names tags, moves and the resolution', async () => {
    const { screen } = await show({
      items: [
        changeItem(change(7, { kind: 'resolved' })),
        changeItem(
          change(6, { kind: 'tags', added: values({ tags: [{ id: 1, name: 'ui', color: 2 }] }) }),
        ),
      ],
      older: null,
      newer: null,
    });

    await expect.element(screen.getByText('Resolved the issue')).toBeVisible();
    expect(screen.container.querySelector('ins')?.textContent).toContain('ui');
  });
});

describe('HIST-002: board records', () => {
  it('names the board and the sprint an issue left and joined', async () => {
    const { screen } = await show({
      items: [
        changeItem(
          change(9, {
            kind: 'boards',
            removed: values({
              boards: [{ board: { id: 1, name: 'Team' }, sprint: { id: 2, name: 'Week 1' } }],
            }),
            added: values({
              boards: [{ board: { id: 1, name: 'Team' }, sprint: { id: 3, name: 'Week 2' } }],
            }),
          }),
        ),
      ],
      older: null,
      newer: null,
    });

    await expect.element(screen.getByText('Boards:')).toBeVisible();
    expect(screen.container.querySelector('del')?.textContent).toContain('Team · Week 1');
    expect(screen.container.querySelector('ins')?.textContent).toContain('Team · Week 2');
  });
});

describe('HIST-003: text changes', () => {
  it('opens the word diff of a description on request', async () => {
    const { screen } = await show({
      items: [
        changeItem(
          change(8, {
            kind: 'description',
            removed: values({ texts: ['The old text'] }),
            added: values({ texts: ['The new text'] }),
          }),
        ),
      ],
      older: null,
      newer: null,
    });

    await expect.element(screen.getByText('Description changed').first()).toBeVisible();
    const toggle = screen.getByRole('button', { name: 'Show differences' });
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'false');
    await toggle.click();

    await expect
      .element(screen.getByRole('button', { name: 'Hide differences' }))
      .toHaveAttribute('aria-expanded', 'true');
    const figure = screen.getByRole('figure');
    await expect.element(figure).toBeVisible();
    expect(figure.element().querySelector('del')?.textContent).toContain('old');
    expect(figure.element().querySelector('ins')?.textContent).toContain('new');
    await expectAccessible(screen.container);
  });
});

describe('HIST-008: the tab strip', () => {
  it('opens a tab, saves the choice in preferences and asks for the stream again', async () => {
    const { screen, calls } = await show(page(), {
      routes: {
        'GET /issues/501/activity': {
          body: { items: [page().items[0], page().items[2]], older: null, newer: null },
        },
      },
    });

    const tabs = screen.getByRole('tablist', { name: 'Shown in the activity' });
    await expect
      .element(tabs.getByRole('tab', { name: 'All', exact: true }))
      .toHaveAttribute('aria-selected', 'true');
    await tabs.getByRole('tab', { name: 'Comments' }).click();

    await expect
      .element(tabs.getByRole('tab', { name: 'Comments' }))
      .toHaveAttribute('aria-selected', 'true');
    expect(preferences.value('activity_comments')).toBe(true);
    expect(preferences.value('activity_changes')).toBe(false);
    await expect.poll(() => texts(screen.container)).toHaveLength(2);
    const request = calls.find((call) => call.path === '/issues/501/activity');
    expect(request?.query.get('changes')).toBe('false');
    expect(request?.query.get('comments')).toBe('true');
  });

  it('arrows walk the tabs and open the one they land on', async () => {
    const { screen } = await show(page());
    const tabs = screen.getByRole('tablist', { name: 'Shown in the activity' });
    const all = tabs.getByRole('tab', { name: 'All', exact: true });

    await all.click();
    (all.element() as HTMLElement).focus();
    await userEvent.keyboard('{ArrowRight}');
    await expect
      .element(tabs.getByRole('tab', { name: 'Comments' }))
      .toHaveAttribute('aria-selected', 'true');
    await userEvent.keyboard('{End}');
    await expect
      .element(tabs.getByRole('tab', { name: 'Changes' }))
      .toHaveAttribute('aria-selected', 'true');
    expect(preferences.value('activity_comments')).toBe(false);
    expect(preferences.value('activity_changes')).toBe(true);
  });

  it('the legacy choice of both filters off reads as «Все»', async () => {
    const { screen } = await show(page(), {
      prefs: { activity_comments: false, activity_changes: false },
    });

    const tabs = screen.getByRole('tablist', { name: 'Shown in the activity' });
    await expect
      .element(tabs.getByRole('tab', { name: 'All', exact: true }))
      .toHaveAttribute('aria-selected', 'true');
    expect(texts(screen.container)).toHaveLength(3);
  });
});

describe('HIST-009: order and long comments', () => {
  function boxBeforeList(container: Element): boolean {
    const form = container.querySelector('form');
    const list = container.querySelector('ol');
    return Boolean(
      form && list && form.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING,
    );
  }

  it('keeps the comment box above the entries in either order', async () => {
    const oldest = await show(page({ items: [...page().items].reverse() }));
    await expect.element(oldest.screen.getByText('First comment')).toBeVisible();
    expect(boxBeforeList(oldest.screen.container)).toBe(true);
    await oldest.screen.unmount();

    const newest = await show(page(), { prefs: { activity_order: 'newest_first' } });
    await expect.element(newest.screen.getByText('First comment')).toBeVisible();
    expect(boxBeforeList(newest.screen.container)).toBe(true);
    expect(texts(newest.screen.container)[0]).toContain('Second comment');
  });

  it('turns a stream loaded whole around without asking the server', async () => {
    const { screen, calls } = await show(page({ items: [...page().items].reverse() }));

    await screen.getByRole('button', { name: 'Oldest first' }).click();

    await expect.element(screen.getByRole('button', { name: 'Newest first' })).toBeVisible();
    expect(preferences.value('activity_order')).toBe('newest_first');
    expect(texts(screen.container)[0]).toContain('Second comment');
    expect(calls.filter((call) => call.path.endsWith('/activity'))).toEqual([]);
  });

  it('cuts a long comment and shows it whole on request', async () => {
    const long = Array.from({ length: 60 }, (_, line) => `<p>Line ${String(line)}</p>`).join('');
    const { screen } = await show({
      items: [commentItem(1, comment(1, 'Long', { html: long }))],
      older: null,
      newer: null,
    });

    const more = screen.getByRole('button', { name: 'Show in full' });
    await expect.element(more).toBeVisible();
    await more.click();
    await expect.element(screen.getByRole('button', { name: 'Collapse' })).toBeVisible();
    await expect.element(screen.getByText('Line 59')).toBeVisible();
  });

  it('keeps long comments whole when the person chose so', async () => {
    const long = Array.from({ length: 60 }, (_, line) => `<p>Line ${String(line)}</p>`).join('');
    const { screen } = await show(
      { items: [commentItem(1, comment(1, 'Long', { html: long }))], older: null, newer: null },
      { prefs: { long_comments: 'full' } },
    );

    await expect.element(screen.getByText('Line 59')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Show in full' }).query()).toBeNull();
  });
});

describe('HIST-010: paging and deep links', () => {
  it('loads earlier items at the older edge', async () => {
    const { screen, calls } = await show(page({ older: 10 }), {
      routes: {
        'GET /issues/501/activity': {
          body: {
            items: [commentItem(5, comment(9, 'Earliest comment'))],
            older: null,
            newer: 10,
          },
        },
      },
    });

    await screen.getByRole('button', { name: 'Show earlier' }).click();

    await expect.element(screen.getByText('Earliest comment')).toBeVisible();
    expect(texts(screen.container)[0]).toContain('Earliest comment');
    const request = calls.find((call) => call.path === '/issues/501/activity');
    expect(request?.query.get('from')).toBe('10');
    expect(request?.query.get('direction')).toBe('older');
    expect(screen.getByRole('button', { name: 'Show earlier' }).query()).toBeNull();
    expect(store.entities.table('comment').get(9)?.text).toBe('Earliest comment');
  });

  it('loads newer items at the newer edge', async () => {
    const { screen, calls } = await show(page({ newer: 30 }), {
      routes: {
        'GET /issues/501/activity': {
          body: { items: [commentItem(40, comment(4, 'Latest comment'))], older: 30, newer: null },
        },
      },
    });

    await screen.getByRole('button', { name: 'Show newer' }).click();

    await expect.element(screen.getByText('Latest comment')).toBeVisible();
    expect(texts(screen.container).at(-1)).toContain('Latest comment');
    expect(calls.find((call) => call.path === '/issues/501/activity')?.query.get('direction')).toBe(
      'newer',
    );
  });

  it("opens the first page in the person's order, or around the comment of a link", async () => {
    const calls = fakeApi({ 'GET /issues/WEB-12/page': { body: issuePage() } });
    setPreferencesForTests({});

    await openIssue('WEB-12');
    expect(calls[0]?.query.get('direction')).toBe('newer');
    const opened = await openIssue('WEB-12', 3);
    expect(calls[1]?.query.get('around_comment')).toBe('3');
    expect(calls[1]?.query.has('direction')).toBe(false);
    expect(opened.kind === 'found' && opened.page.focusComment).toBe(3);
  });

  it('CNT-007: scrolls to, focuses and highlights the comment of the link', async () => {
    const { screen } = await show(page({ older: 10, newer: 30 }), { focus: 1 });

    // The page around a comment comes newest first, as other pages going `older`.
    await expect.element(screen.getByText('First comment')).toBeVisible();
    const card = screen.container.querySelector('#comment-1');
    await expect.poll(() => document.activeElement).toBe(card);
    expect(card?.classList.contains('focused')).toBe(true);
    expect(texts(screen.container)[0]).toContain('First comment');
    await expect.element(screen.getByRole('button', { name: 'Show earlier' })).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Show newer' })).toBeVisible();
  });
});

describe('HIST-011: changes made with a comment', () => {
  // Oldest first, as the default order asks the server for.
  const grouped = (): ActivityPage => ({
    items: [
      commentItem(10, comment(1, 'Fixed it')),
      changeItem(
        change(11, {
          author: comment(1, '').author,
          after_comment: 1,
          field: PRIORITY.field,
          added: values({ values: [CRITICAL] }),
        }),
      ),
    ],
    older: null,
    newer: null,
  });

  it('shows them inside the comment', async () => {
    const { screen } = await show(grouped());

    const card = screen.getByRole('article', { name: 'Comment by Anna Admin' });
    await expect
      .element(card.getByRole('list', { name: 'Changes with the comment' }))
      .toHaveTextContent('Priority: added: Critical');
    expect(texts(screen.container)).toHaveLength(1);
  });

  it('shows them on their own when comments are hidden', async () => {
    const { screen } = await show(
      { ...grouped(), items: grouped().items.slice(1) },
      { prefs: { activity_comments: false } },
    );

    await expect.element(screen.getByText('Critical')).toBeVisible();
    expect(screen.getByRole('article').query()).toBeNull();
  });
});
