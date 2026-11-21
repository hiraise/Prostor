// The issue page in read form on a desktop and on a phone (step 1.7a; ISS-001, ISS-003, ISS-016,
// ISS-020, FLD-057, FLD-062, UI-005) and its keys (step 2.2 §7, ISS-047).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Attachment, Comment, Issue, Session } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setSessionForTests } from '#lib/session/index.ts';
import { preferences, setAccessForTests, setPreferencesForTests } from '#lib/session/index.ts';
import {
  dispatchClaim,
  focusedEntity,
  resetShortcutsForTests,
} from '#lib/shortcuts/registry.svelte.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi } from '../../../../test/fake-api.ts';
import { recentCommands } from '../commands/recent.svelte.ts';
import { issueCreateExtra } from '../new/create-extra.svelte.ts';
import { ASSIGNEE, PRIORITY, field, issue, issuePage, ANNA, BORIS } from '../test-data.ts';
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

/** Comments, attaches and creates issues in project 1 — the keys that need the rights. */
const WRITER: AccessMap = {
  global: [],
  projects: [
    { project_id: 1, permissions: ['project.basic', 'comment.create', 'attachment.create'] },
  ],
  capabilities: [],
};

/** The signed-in Anna of the `m`, `r` and `f` keys. */
const SESSION = {
  created_at: '2026-11-14T09:00:00Z',
  password_change_required: false,
  remember: false,
  user: { ...ANNA, email: null, email_verified: true, version: 1 },
} as unknown as Session;

function show(shown: Issue, access: AccessMap = READER, requested?: string) {
  setAccessForTests(access);
  setPreferencesForTests({});
  return render(IssuePage, { issue: shown, requested });
}

async function openFieldsOnPhone(): Promise<void> {
  if (isPhone()) await page.getByText('Fields', { exact: true }).click();
}

afterEach(() => {
  vi.unstubAllGlobals();
  navigation.goto.mockClear();
  store.entities.table('issue').clear();
  store.entities.table('comment').clear();
  store.entities.table('attachment').clear();
  forgetIssuePages();
  recentCommands.reset();
  resetShortcutsForTests();
  setSessionForTests(null);
  setAccessForTests(null);
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
});

describe('IssuePage', () => {
  it('CNT-041: ticks a checklist box of the description and saves the text', async () => {
    const html = (ticked: boolean): string =>
      [
        '<p>Plan:</p>',
        '<ul>',
        `<li data-sourcepos="2:1-2:11"><input type="checkbox"${ticked ? ' checked=""' : ''} disabled=""> write</li>`,
        '</ul>',
        '',
      ].join('\n');
    const shown = issue({ description: 'Plan:\n- [ ] write\n', description_html: html(false) });
    const calls = fakeApi({
      'PATCH /issues/501': ({ body }) => ({
        body: {
          issue: {
            ...shown,
            ...(body as object),
            description_html: html((body as { description: string }).description.includes('- [x]')),
            text_version: 3,
          },
          messages: [],
        },
      }),
    });
    // The save runs through the store's row of the issue (a refusal would put the box back).
    store.entities.table('issue').upsert(shown, 'full');
    const screen = await show(shown);
    const box = screen.getByRole('checkbox', { name: 'write' });
    await expect.element(box).toBeEnabled();
    await box.click();
    await expect.element(box).toBeChecked();
    expect(calls.find((one) => one.method === 'PATCH')?.body).toMatchObject({
      description: 'Plan:\n- [x] write\n',
      text_version: 2,
    });
  });
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
    // The issue's visibility sits behind the lock beside «Изменить».
    const lock = screen.getByRole('button', { name: 'Visibility', exact: true });
    await expect.element(lock).toHaveAttribute('aria-pressed', 'false');
    await lock.click();
    const visibility = screen.getByRole('dialog', { name: 'Visibility' });
    await expect
      .element(visibility.getByRole('radio', { name: 'Everyone who sees the project' }))
      .toBeChecked();
    await userEvent.keyboard('{Escape}');
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
    await openFieldsOnPhone();
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

    // A restricted issue: the lock is on, and its popover names the audience with the reporter.
    const lock = screen.getByRole('button', { name: 'Visibility', exact: true });
    await expect.element(lock).toHaveAttribute('aria-pressed', 'true');
    await lock.click();
    const visibility = screen.getByRole('dialog', { name: 'Visibility' });
    await expect.element(visibility.getByRole('radio', { name: 'Only the chosen' })).toBeChecked();
    // The audience with the reporter: ISS-041's words live in the picker now.
    await expect.element(visibility.getByRole('checkbox', { name: /Developers/ })).toBeChecked();
    await expect.element(visibility.getByRole('checkbox', { name: /Boris Lead/ })).toBeChecked();
  });

  it('ISS-020: the panel shows the project first, then the fields in order', async () => {
    const screen = await show(issue(), LEAD);
    await openFieldsOnPhone();
    const panel = screen.getByRole('complementary', { name: 'Fields' });

    const names = panel
      .getByRole('term')
      .elements()
      .map((term) => term.textContent.trim());
    expect(names).toEqual(['Project', 'Priority', 'Assignee', 'Estimation', 'Due date']);
    // The project's value opens the move dropdown; its settings live in the project itself.
    const project = panel.getByRole('button', { name: 'Web' });
    await expect.element(project).toBeVisible();
    expect(project.element().textContent).toContain('Web');
    await expect.element(panel.getByText('Major')).toBeVisible();
    await expect.element(panel.getByText('Boris Lead')).toBeVisible();
    await expect.element(panel.getByText('1h 30m')).toBeVisible();
    expect(panel.element().querySelector('time')?.getAttribute('datetime')).toBe('2026-11-02');
  });

  it('ISS-020: a reader gets a link to the project, no dropdown; empty fields show their label', async () => {
    const screen = await show(
      issue({
        fields: [field(12, 'Assignee', 'user', { empty_label: 'Unassigned' })],
        can: { ...issue().can, update: false },
      }),
    );
    await openFieldsOnPhone();
    const panel = screen.getByRole('complementary', { name: 'Fields' });

    await expect.element(panel.getByText('Unassigned')).toBeVisible();
    expect(panel.getByRole('link', { name: 'Web' }).element().getAttribute('href')).toMatch(
      /\/projects\/WEB$/,
    );
    expect(panel.getByRole('button', { name: 'Web' }).query()).toBeNull();
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
    await openFieldsOnPhone();
    const panel = screen.getByRole('complementary', { name: 'Fields' });

    await expect.element(panel.getByText('Required field')).toBeVisible();
    await expect.element(panel.getByRole('img', { name: 'Hidden field' })).toBeVisible();
    await expect.element(panel.getByText('1,200')).toBeVisible();
    expect(panel.getByRole('img', { name: 'Hidden field' }).elements()).toHaveLength(1);
  });

  it('ISS-020: side and width come from personal settings (desktop)', async () => {
    if (isPhone()) return;
    setAccessForTests(READER);
    setPreferencesForTests({ issue_panel_side: 'start', issue_panel_width: 300 });
    const screen = await render(IssuePage, { issue: issue() });
    await openFieldsOnPhone();
    const panel = screen.getByRole('complementary', { name: 'Fields' });
    const content = screen.getByRole('region', { name: 'Description' });

    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
    const panelFrame = panel.element().parentElement;
    if (!panelFrame) throw new Error('no field panel frame');
    expect(panelFrame.getBoundingClientRect().width).toBeCloseTo(30 * rem, 0);
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
  });

  it('ISS-020: on phones the fields open between the title and the content', async () => {
    if (!isPhone()) return;
    setAccessForTests(READER);
    const screen = await render(IssuePage, { issue: issue() });
    await openFieldsOnPhone();
    const panel = screen.getByRole('complementary', { name: 'Fields' });

    // No heading of its own: the properties start at once.
    expect(panel.getByRole('heading').query()).toBeNull();
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

describe('ISS-047: the keys of the issue view', () => {
  /** Renders the page as Anna, with her page data, the store loaded and the API the keys touch. */
  async function keys(shown: Issue = issue(), routes: Record<string, unknown> = {}) {
    const calls = fakeApi({
      'PATCH /users/me/preferences': { body: {} },
      'POST /commands/assist': { body: { operations: [], problems: [], suggestions: [] } },
      'GET /issues/501/activity': { body: { items: [], older: null, newer: null } },
      ...routes,
    });
    setSessionForTests(SESSION);
    setAccessForTests(WRITER);
    setPreferencesForTests({});
    store.entities.table('issue').upsert(shown, 'full');
    const screen = await render(IssuePage, { issue: shown, page: issuePage(shown) });
    // Keys need the frame's focus: another file's iframe may hold it in a full run.
    await screen.getByRole('heading', { level: 1 }).click();
    return { screen, calls };
  }

  it('`e` opens the edit form, Escape closes it', async () => {
    const { screen } = await keys();
    await userEvent.keyboard('e');
    await expect.element(screen.getByRole('textbox', { name: 'Summary' })).toBeVisible();
    await userEvent.keyboard('{Escape}');
    await expect.element(screen.getByRole('textbox', { name: 'Summary' }).query()).toBeNull();
  });

  it('`r` opens the comment box with the caret in the text', async () => {
    const { screen } = await keys();
    await userEvent.keyboard('r');
    const form = screen.getByRole('form', { name: 'New comment' });
    await expect.element(form).toBeVisible();
    expect(document.activeElement).toBe(form.element().querySelector('textarea'));
  });

  it('`h` and `u` toggle history and comments in the activity stream', async () => {
    const { calls } = await keys();
    await userEvent.keyboard('h');
    expect(preferences.value('activity_changes')).toBe(false);
    await userEvent.keyboard('h');
    expect(preferences.value('activity_changes')).toBe(true);
    await userEvent.keyboard('u');
    expect(preferences.value('activity_comments')).toBe(false);
    await userEvent.keyboard('u');
    expect(preferences.value('activity_comments')).toBe(true);
    // Each toggle reloads the stream with the new tab's filters.
    expect(calls.filter((call) => call.path === '/issues/501/activity')).toHaveLength(4);
  });

  it('`m` assigns to me, `s` takes the star off', async () => {
    // `m` writes the builtin assignee field, so the row carries it.
    const withAssignee = issue({
      fields: [PRIORITY, { ...ASSIGNEE, field: { ...ASSIGNEE.field, builtin: 'assignee' } }],
    });
    const { calls } = await keys(withAssignee, {
      'PUT /issues/501/fields/12': { status: 204 },
      'DELETE /issues/501/star': { status: 204 },
    });
    await userEvent.keyboard('s');
    await userEvent.keyboard('m');
    await vi.waitFor(() => {
      expect(calls.map((call) => [call.method, call.path])).toEqual(
        expect.arrayContaining([
          ['DELETE', '/issues/501/star'],
          ['PUT', '/issues/501/fields/12'],
        ]),
      );
    });
    expect(calls.find((call) => call.path === '/issues/501/fields/12')?.body).toEqual({ value: 1 });
  });

  it('`f` opens the file picker, `t` the tag picker', async () => {
    const { screen } = await keys();
    const picker = screen.container.querySelector('input[type="file"]');
    expect(picker).not.toBeNull();
    let clicked = 0;
    picker?.addEventListener('click', () => {
      clicked += 1;
    });
    await userEvent.keyboard('f');
    expect(clicked).toBe(1);
    await userEvent.keyboard('t');
    await expect.element(screen.getByRole('dialog', { name: 'Add tag' })).toBeVisible();
  });

  it('the open issue is the focused entity, and «Создать» gains its entries', async () => {
    const shown = issue();
    setSessionForTests(SESSION);
    setAccessForTests({
      global: [],
      projects: [{ project_id: 1, permissions: ['project.basic', 'issue.create'] }],
      capabilities: [],
    });
    setPreferencesForTests({});
    fakeApi({});
    await render(IssuePage, {
      issue: shown,
      page: issuePage(shown, { links: { groups: [], can_link: true } }),
    });
    const entity = focusedEntity();
    expect(entity?.id).toBe('WEB-12');
    expect(entity?.summary).toBe(shown.summary);
    expect(entity?.href).toContain('WEB-12');
    expect(issueCreateExtra().map((entry) => (entry.type === 'action' ? entry.label : ''))).toEqual(
      ['New subtask', 'New linked issue…'],
    );
  });

  it('the command claim opens the dialog with the typed letter; the edit form refuses it', async () => {
    const { screen } = await keys();
    expect(dispatchClaim('command', 'q')).toBe(true);
    const dialog = screen.getByRole('dialog', { name: 'Commands' });
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog.getByRole('combobox')).toHaveValue('q');
    await userEvent.keyboard('{Escape}');
    await expect.element(dialog.query()).toBeNull();

    // While the edit form is shown, a letter never opens the dialog over it (the rule of 2.1).
    await userEvent.keyboard('e');
    expect(dispatchClaim('command', 'z')).toBe(false);
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
