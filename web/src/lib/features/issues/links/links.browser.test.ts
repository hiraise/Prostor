// Links and subtasks on the issue page (step 1.7b-5; LNK-005, LNK-006, LNK-008, LNK-010, LNK-013,
// LNK-014, LNK-015, ISS-009). The new-issue form's side (LNK-007, ISS-009) is in
// new/new-issue.browser.test.ts.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  AccessMap,
  Issue,
  IssueBrief,
  IssueLinks,
  LinkGroup,
  LinkType,
} from '#lib/api/schema.gen.ts';
import { preferences, setAccessForTests, setPreferencesForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { NARROW } from '#lib/theme/breakpoints.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { forgetIssuePages } from '../issues.ts';
import { newIssueDialog } from '../new/open.svelte.ts';
import IssuePage from '../page/IssuePage.svelte';
import { issue, issuePage, WEB } from '../test-data.ts';
import { forgetLinkTypes } from './links.svelte.ts';

const navigation = vi.hoisted(() => ({
  goto: vi.fn<(href: string, options?: unknown) => Promise<void>>(() => Promise.resolve()),
}));
vi.mock('$app/navigation', () => navigation);

/** May link, create and change issues in project 1. */
const LINKER: AccessMap = {
  global: [],
  projects: [
    {
      project_id: 1,
      permissions: ['project.basic', 'issue.update', 'issue.link', 'issue.create'],
    },
  ],
  capabilities: [],
};

function type(
  id: number,
  source: string,
  target: string | null,
  rest: Partial<LinkType> = {},
): LinkType {
  return {
    id,
    name: source,
    source_name: source,
    target_name: target,
    direction: target === null ? 'undirected' : 'directed',
    builtin: null,
    locked: false,
    deleted_at: null,
    usage: null,
    version: 1,
    ...rest,
  };
}

const RELATED = type(1, 'related to', null, { builtin: 'related' });
const DEPENDS = type(2, 'blocks', 'blocked by', { builtin: 'dependency' });
const DUPLICATE = type(3, 'has duplicate', 'duplicate of', {
  builtin: 'duplicate',
  direction: 'hierarchy',
});
const SUBTASK = type(4, 'parent of', 'child of', { builtin: 'subtask', direction: 'hierarchy' });
const TYPES = [RELATED, DEPENDS, DUPLICATE, SUBTASK];

function brief(id: number, summary: string, resolved = false): IssueBrief {
  return {
    id,
    readable_id: `WEB-${String(id - 500)}`,
    summary,
    project: WEB,
    resolved,
    restricted: false,
  };
}

function other(id: number, summary: string, rest: Partial<Issue> = {}): Issue {
  return issue({ id, readable_id: `WEB-${String(id - 500)}`, summary, starred: false, ...rest });
}

function group(linkType: LinkType, side: 'source' | 'target', items: IssueBrief[], rest = {}) {
  return {
    link_type: {
      id: linkType.id,
      name: linkType.name,
      source_name: linkType.source_name,
      target_name: linkType.target_name,
      direction: linkType.direction,
      builtin: linkType.builtin,
    },
    side,
    name: side === 'source' ? linkType.source_name : (linkType.target_name ?? ''),
    items: items.map((linked) => ({
      link: 70 + linked.id - 500,
      issue: linked,
      linked_at: '2026-10-22T10:00:00Z',
    })),
    total: items.length,
    unresolved: items.filter((linked) => !linked.resolved).length,
    next_cursor: null,
    ...rest,
  } satisfies LinkGroup;
}

const CHILD_A = brief(502, 'Fix the handler');
const CHILD_B = brief(503, 'Add a test', true);
const CHILD_C = brief(504, 'Update docs');
const NEAR = brief(505, 'Login page is slow');

const LINKS: IssueLinks = {
  can_link: true,
  groups: [
    group(SUBTASK, 'source', [CHILD_A, CHILD_B], { total: 3, unresolved: 2, next_cursor: 'c1' }),
    group(RELATED, 'source', [NEAR]),
  ],
};

async function show(
  links: IssueLinks = LINKS,
  routes: Record<string, FakeRoute> = {},
  rest: Partial<Issue> = {},
) {
  const shown = issue({ subtasks: { total: 3, unresolved: 1 }, ...rest });
  const calls = fakeApi({
    'PATCH /users/me/preferences': { body: {} },
    'GET /link-types': { body: { items: TYPES } },
    'POST /issues/batch': ({ body }) => ({
      body: {
        items: (body as { ids: number[] }).ids.map((id) => other(id, `Issue ${String(id)}`)),
      },
    }),
    ...routes,
  });
  setAccessForTests(LINKER);
  setPreferencesForTests({});
  store.put('issue', [shown], 'full');
  const answer = issuePage(shown, { links });
  const screen = await render(IssuePage, {
    issue: shown,
    page: { activity: answer.activity, links: answer.links, attachments: answer.attachments },
  });
  return { screen, calls };
}

function sent(calls: FakeRequest[]) {
  return calls
    .filter((call) => call.method !== 'GET' && call.path !== '/issues/batch')
    .filter((call) => call.path !== '/users/me/preferences')
    .map((call) => [call.method, call.path, call.body]);
}

const narrow = () => matchMedia(NARROW).matches;

/** «Ещё» of the head, then one of its items. */
async function fromMenu(item: string) {
  await page.getByRole('button', { name: 'More', exact: true }).click();
  await page.getByRole('menuitem', { name: item }).click();
}

/** «Связать»: the link icon in the title row on every width. */
async function openLinkDialog() {
  await page.getByRole('button', { name: 'Link', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Link issues' });
  await expect.element(dialog).toBeVisible();
  return dialog;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  navigation.goto.mockClear();
  newIssueDialog.open = false;
  store.entities.table('issue').clear();
  forgetIssuePages();
  forgetLinkTypes();
  toasts.clear();
  setAccessForTests(null);
  setPreferencesForTests(null);
});

describe('LNK-010: the links panel', () => {
  it('shows tabs by link name, rows with stars, more rows and the list link; folds', async () => {
    const { screen, calls } = await show(LINKS, {
      'GET /issues/501/links/4/source': {
        body: group(SUBTASK, 'source', [CHILD_C], { total: 3, unresolved: 2 }),
      },
      'PUT /issues/502/star': { status: 204 },
    });

    const panel = screen.getByRole('region', { name: /^Links/ });
    const tabs = panel.getByRole('tablist', { name: 'Link names' });
    await expect
      .element(tabs.getByRole('tab', { name: 'All 4' }))
      .toHaveAttribute('aria-selected', 'true');
    await expect.element(panel.getByRole('list', { name: 'parent of' })).toBeVisible();
    await expect.element(panel.getByRole('list', { name: 'related to' })).toBeVisible();
    const list = panel.getByRole('link', { name: 'Open as a list' });
    await expect
      .element(list)
      .toHaveAttribute('href', `/issues?q=${encodeURIComponent('links: WEB-12')}`);
    await expect.element(list).toHaveAttribute('target', '_blank');

    // Stars come with the linked issues' rows.
    const star = panel.getByRole('button', { name: 'Watch WEB-2' });
    await expect.element(star).toHaveAttribute('aria-pressed', 'false');
    await star.click();
    await expect.element(star).toHaveAttribute('aria-pressed', 'true');
    expect(sent(calls)).toContainEqual(['PUT', '/issues/502/star', undefined]);

    await panel.getByRole('button', { name: 'Show more' }).click();
    await expect.element(panel.getByRole('link', { name: /WEB-4/ })).toBeVisible();
    await expect.element(panel.getByRole('button', { name: 'Show more' })).not.toBeInTheDocument();

    // A tab shows its group; arrows move between tabs.
    await tabs.getByRole('tab', { name: 'related to 1' }).click();
    await expect.element(panel.getByRole('list', { name: 'parent of' })).not.toBeInTheDocument();
    await expect
      .element(list)
      .toHaveAttribute('href', `/issues?q=${encodeURIComponent('related to: WEB-12')}`);
    await userEvent.keyboard('{ArrowLeft}');
    await expect.element(tabs.getByRole('tab', { name: 'parent of 3' })).toHaveFocus();
    await expect
      .element(list)
      .toHaveAttribute('href', `/issues?q=${encodeURIComponent('child of: WEB-12')}`);

    await expectAccessible(panel.element());
    if (isPhone()) expect(smallTargets(panel.element())).toEqual([]);

    const toggle = panel.getByRole('button', { name: 'Links 4' });
    await toggle.click();
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect.element(tabs).not.toBeInTheDocument();
    expect(preferences.value('links_collapsed')).toBe(true);
  });

  it('is not shown without links; a reader without the right gets no remove buttons', async () => {
    const empty = await show({ groups: [], can_link: true });
    await expect.element(empty.screen.getByText('Login button does nothing')).toBeVisible();
    expect(empty.screen.container.textContent).not.toContain('Links');
    await empty.screen.unmount();

    const { screen } = await show({ ...LINKS, can_link: false });
    const panel = screen.getByRole('region', { name: /^Links/ });
    await expect.element(panel.getByRole('link', { name: /WEB-2/ })).toBeVisible();
    expect(panel.getByRole('button', { name: /^Remove the link/ }).elements()).toEqual([]);
  });
});

describe('LNK-008: remove a link', () => {
  it('removes the row and tells the server', async () => {
    const { screen, calls } = await show(LINKS, {
      'DELETE /issues/501/links/75': { status: 200, body: { messages: [] } },
    });

    const panel = screen.getByRole('region', { name: /^Links/ });
    await panel.getByRole('button', { name: 'Remove the link to WEB-5' }).click();
    await expect.element(panel.getByRole('list', { name: 'related to' })).not.toBeInTheDocument();
    await expect.element(panel.getByRole('tab', { name: 'All 3' })).toBeVisible();
    expect(sent(calls)).toEqual([['DELETE', '/issues/501/links/75', undefined]]);
  });
});

describe('LNK-013, LNK-014: parent and subtask progress', () => {
  it('names the parent under the header and counts resolved subtasks', async () => {
    const { screen } = await show(LINKS, {}, { parent: brief(500, 'Login epic') });

    await expect.element(screen.getByText('Subtask of')).toBeVisible();
    await expect.element(screen.getByRole('link', { name: /WEB-0 Login epic/ })).toBeVisible();
    await expect.element(screen.getByText('2 of 3 resolved')).toBeVisible();
  });
});

describe('LNK-005: add links', () => {
  it('offers recent issues, finds others, links the chosen ones and names existing pairs', async () => {
    const { calls } = await show(LINKS, {
      'GET /issues': ({ query }) => ({
        body: {
          items: query.get('q') === 'login' ? [other(506, 'Login form'), other(501, 'Self')] : [],
          next_cursor: null,
          live_topic: 'issues',
          notes: [],
          query: query.get('q') ?? '',
          single_id: null,
          sort: [],
        },
      }),
      'POST /issues/501/links': {
        body: {
          messages: [],
          added: [507],
          existing: [506],
          links: {
            ...LINKS,
            groups: [...LINKS.groups, group(DEPENDS, 'source', [brief(507, 'X')])],
          },
        },
      },
    });
    preferences.set({ recent_issues: [501, 507] });

    const dialog = await openLinkDialog();
    const name = dialog.getByRole('combobox', { name: 'Link' });
    await expect.element(name).toHaveValue('1:source');
    await name.selectOptions('blocks');
    // Before typing: the recent issues, without this one.
    await dialog.getByRole('checkbox', { name: /WEB-7/ }).click();
    await dialog.getByRole('searchbox', { name: 'Find an issue' }).fill('login');
    await dialog.getByRole('checkbox', { name: /WEB-6/ }).click();
    expect(dialog.getByRole('checkbox', { name: /Self/ }).elements()).toEqual([]);
    await expectAccessible(dialog.element());

    await dialog.getByRole('button', { name: 'Link 2' }).click();
    await expect.element(dialog.getByText('Already linked this way: WEB-6')).toBeVisible();
    expect(sent(calls)).toEqual([
      ['POST', '/issues/501/links', { link_type: 2, side: 'source', issues: [507, 506] }],
    ]);
    await expect.element(page.getByRole('list', { name: 'blocks' })).toBeVisible();
  });
});

describe('LNK-006: create and link', () => {
  it('creates an issue in this project already linked, or opens the full form', async () => {
    const { calls } = await show(LINKS, {
      'GET /issues': { body: { items: [], next_cursor: null } },
      'POST /issues': {
        status: 201,
        body: { issue: other(508, 'Write the release note'), messages: [] },
      },
      'GET /issues/501/links': { body: LINKS },
    });

    let dialog = await openLinkDialog();
    await dialog.getByRole('searchbox', { name: 'Find an issue' }).fill('Write the release note');
    await dialog
      .getByRole('button', { name: 'Create issue “Write the release note” and link' })
      .click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(sent(calls)).toEqual([
      [
        'POST',
        '/issues',
        {
          project: 1,
          summary: 'Write the release note',
          links: [{ issue: 501, link_type: 1, side: 'source' }],
        },
      ],
    ]);

    dialog = await openLinkDialog();
    await dialog.getByRole('combobox', { name: 'Link' }).selectOptions('blocked by');
    await dialog.getByRole('button', { name: 'Open the full form' }).click();
    const link = { issue: 501, link_type: 2, side: 'source' };
    if (narrow()) {
      expect(navigation.goto).toHaveBeenCalledWith('/issues/new?project=WEB&link=2%3Asource%3A501');
    } else {
      expect(newIssueDialog.target).toEqual({ project: 'WEB', link });
    }
  });
});

describe('LNK-015: mark as duplicate', () => {
  it('opens the link dialog with the duplicate name chosen', async () => {
    await show();

    await fromMenu('Mark as duplicate…');
    const dialog = page.getByRole('dialog', { name: 'Mark as duplicate' });
    await expect.element(dialog.getByRole('combobox', { name: 'Link' })).toHaveValue('3:target');
  });
});

describe('ISS-009: new subtask or linked issue', () => {
  it('starts the new-issue form as a subtask, or linked by a chosen name', async () => {
    await show();

    await fromMenu('New subtask');
    const subtask = { issue: 501, link_type: 4, side: 'target' };
    await vi.waitFor(() => {
      if (narrow()) {
        expect(navigation.goto).toHaveBeenCalledWith(
          '/issues/new?project=WEB&link=4%3Atarget%3A501',
        );
      } else {
        expect(newIssueDialog.target).toEqual({ project: 'WEB', link: subtask });
      }
    });
    newIssueDialog.open = false;

    await fromMenu('New linked issue…');
    const dialog = page.getByRole('dialog', { name: 'New linked issue' });
    // Only the name is chosen here.
    expect(dialog.getByRole('searchbox').elements()).toEqual([]);
    await dialog.getByRole('combobox', { name: 'Link' }).selectOptions('blocks');
    await dialog.getByRole('button', { name: 'Open the full form' }).click();
    const blocked = { issue: 501, link_type: 2, side: 'target' };
    if (narrow()) {
      expect(navigation.goto).toHaveBeenLastCalledWith(
        '/issues/new?project=WEB&link=2%3Atarget%3A501',
      );
    } else {
      expect(newIssueDialog.target).toEqual({ project: 'WEB', link: blocked });
    }
  });
});
