// Changing an issue from its page on a desktop and on a phone (step 1.7a-4; ISS-018, ISS-020,
// ISS-028, ISS-030, ISS-032, ISS-034, ISS-035, ISS-051, UI-006).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Issue, Session, Tag } from '#lib/api/schema.gen.ts';
import {
  setAccessForTests,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import IssuePageHarness from '../../../../test/harness/IssuePageHarness.svelte';
import { resetCloning } from '../clone.svelte.ts';
import { ANNA, BORIS, field, issue, WEB } from '../test-data.ts';
import IssueActions from './IssueActions.svelte';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

const ID = 501;

/** Sees projects 1 and 2 and creates issues in both. */
const MEMBER: AccessMap = {
  global: [],
  projects: [1, 2, 3].map((project_id) => ({
    project_id,
    permissions: ['project.basic', 'issue.create', 'issue.update'],
  })),
  capabilities: [],
};

const NOTES = field(15, 'Steps to reproduce', 'text', {
  text: 'Open the page',
  html: '<p>Open the page</p>\n',
});

function tag(id: number, name: string, apply = true): Tag {
  return {
    id,
    name,
    color: 4,
    owner: ANNA,
    sharing: null,
    abilities: { apply, edit: false, delete: false, share: false, transfer: false },
    version: 1,
    created_at: '2026-10-01T00:00:00Z',
  };
}

async function show(
  shown: Issue = issue({ fields: [NOTES] }),
  routes: Record<string, FakeRoute> = {},
) {
  const calls = fakeApi(routes);
  store.entities.table('issue').upsert(shown, 'full');
  const screen = await render(IssuePageHarness, { id: ID });
  return { screen, calls };
}

function sent(calls: FakeRequest[], method?: string) {
  return calls
    .filter((call) => call.method !== 'GET' && (!method || call.method === method))
    .map((call) => [call.method, call.path, call.body]);
}

const summaryBox = () => page.getByRole('textbox', { name: /^Summary/ });

beforeEach(() => {
  setAccessForTests(MEMBER);
  setPreferencesForTests({});
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  navigation.goto.mockClear();
  store.entities.table('issue').clear();
  toasts.clear();
  resetCloning();
  setAccessForTests(null);
  setPreferencesForTests(null);
  setSessionForTests(null);
});

describe('ISS-018: edit mode', () => {
  it('F2 opens it; Ctrl+Enter saves the changed texts on the version editing began with', async () => {
    const { screen, calls } = await show(issue({ fields: [NOTES] }), {
      'PATCH /issues/501': ({ body }) => ({
        body: {
          issue: issue({ ...(body as Partial<Issue>), fields: [NOTES], text_version: 3 }),
          messages: [],
        },
      }),
      'PUT /issues/501/fields/15': ({ body }) => ({
        body: {
          issue: issue({
            summary: 'Login button works',
            text_version: 3,
            fields: [{ ...NOTES, text: (body as { value: string }).value }],
          }),
          messages: [],
        },
      }),
    });

    await userEvent.keyboard('{F2}');
    const form = screen.getByRole('form', { name: 'Edit the issue' });
    await expect.element(form).toBeVisible();
    await expect.element(summaryBox()).toHaveValue('Login button does nothing');
    await expectAccessible(form.element());
    if (isPhone()) expect(smallTargets(form.element())).toEqual([]);

    await userEvent.clear(summaryBox());
    await userEvent.type(summaryBox(), 'Login button works');
    const notes = form.getByRole('textbox', { name: /^Steps to reproduce/ });
    await userEvent.clear(notes);
    await userEvent.type(notes, 'Click it');
    await userEvent.keyboard('{Control>}{Enter}{/Control}');

    await expect.element(form).not.toBeInTheDocument();
    expect(sent(calls)).toEqual([
      ['PATCH', '/issues/501', { summary: 'Login button works', text_version: 2 }],
      ['PUT', '/issues/501/fields/15', { value: 'Click it' }],
    ]);
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Login button works' }))
      .toBeVisible();
  });

  it('Esc leaves at once without changes and asks first with them', async () => {
    const { screen, calls } = await show();

    await screen.getByRole('button', { name: 'Edit', exact: true }).click();
    await expect.element(summaryBox()).toBeVisible();
    await userEvent.keyboard('{Escape}');
    await expect.element(summaryBox()).not.toBeInTheDocument();

    await screen.getByRole('button', { name: 'Edit', exact: true }).click();
    await userEvent.type(summaryBox(), ' now');
    await userEvent.keyboard('{Escape}');
    const confirm = page.getByRole('dialog', { name: 'Discard your edits?' });
    await expect.element(confirm).toBeVisible();
    await confirm.getByRole('button', { name: 'Keep editing' }).click();
    await expect.element(summaryBox()).toHaveValue('Login button does nothing now');

    await screen.getByRole('button', { name: 'Cancel' }).click();
    await confirm.getByRole('button', { name: 'Discard edits' }).click();
    await expect.element(summaryBox()).not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Login button does nothing' }))
      .toBeVisible();
    expect(sent(calls)).toEqual([]);
  });

  it('a summary is required; visibility narrows to the chosen, the reporter always in', async () => {
    const { screen, calls } = await show(issue({ fields: [] }), {
      'GET /projects/1/audience-candidates': {
        body: {
          groups: [{ id: 7, name: 'Developers', builtin: null, team_of: null }],
          users: [ANNA, BORIS],
        },
      },
      'PATCH /issues/501': ({ body }) => ({
        body: {
          issue: issue({
            fields: [],
            ...(body as object),
            audience: {
              users: [],
              groups: [{ id: 7, name: 'Developers', builtin: null, team_of: null }],
            },
          }),
          messages: [],
        },
      }),
    });

    await screen.getByRole('button', { name: 'Edit', exact: true }).click();
    await userEvent.clear(summaryBox());
    await screen.getByRole('button', { name: 'Save' }).click();
    await expect.element(page.getByText('An issue needs a summary.')).toBeVisible();
    await expect.element(summaryBox()).toHaveFocus();
    await userEvent.type(summaryBox(), 'Login button does nothing');

    await screen.getByRole('radio', { name: 'Only the chosen' }).click();
    await screen.getByRole('button', { name: 'Save' }).click();
    await expect.element(page.getByText('Choose at least one group or person.')).toBeVisible();
    const reporter = screen.getByRole('checkbox', { name: /Anna Admin/ });
    await expect.element(reporter).toBeChecked();
    await expect.element(reporter).toBeDisabled();
    await screen.getByRole('checkbox', { name: /Developers/ }).click();
    await screen.getByRole('button', { name: 'Save' }).click();

    await expect.element(summaryBox()).not.toBeInTheDocument();
    expect(sent(calls)).toEqual([
      ['PATCH', '/issues/501', { audience: { users: [], groups: [7] } }],
    ]);
    await expect.element(screen.getByText('Restricted', { exact: true })).toBeVisible();
  });
});

describe('ISS-051: a text conflict', () => {
  const theirs = issue({ summary: 'Login button is broken', fields: [], text_version: 3 });

  async function conflict() {
    let patches = 0;
    const shown = await show(issue({ fields: [] }), {
      'PATCH /issues/501': ({ body }) => {
        patches += 1;
        const { text_version } = body as { text_version: number };
        if (text_version < 3) return failure(409, 'conflict');
        return {
          body: {
            issue: issue({ fields: [], ...(body as object), text_version: 4 }),
            messages: [],
          },
        };
      },
      'GET /issues/501': { body: theirs },
    });
    await shown.screen.getByRole('button', { name: 'Edit', exact: true }).click();
    await userEvent.clear(summaryBox());
    await userEvent.type(summaryBox(), 'Login button needs a handler');
    await shown.screen.getByRole('button', { name: 'Save' }).click();
    const dialog = page.getByRole('dialog', { name: 'Someone changed this issue' });
    await expect.element(dialog).toBeVisible();
    return { ...shown, dialog, patches: () => patches };
  }

  it('opens the conflict view with the word diff; «Save my version» saves on the new version', async () => {
    const { dialog, calls, screen, patches } = await conflict();

    expect(patches()).toBe(1);
    const diff = dialog.getByRole('figure', { name: 'Summary' });
    await expect.element(diff.getByRole('deletion')).toHaveTextContent('removed: is broken');
    await expect.element(diff.getByRole('insertion')).toHaveTextContent('added: needs a handler');
    await expectAccessible(dialog.element());

    await dialog.getByRole('button', { name: 'Save my version' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(sent(calls).at(-1)).toEqual([
      'PATCH',
      '/issues/501',
      { summary: 'Login button needs a handler', text_version: 3 },
    ]);
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Login button needs a handler' }))
      .toBeVisible();
  });

  it('is never resolved silently: a retry by the store keeps the old version and is refused', async () => {
    // Someone changed only the description: the summary looks untouched to the store.
    const quiet = issue({ description: 'Changed meanwhile', fields: [], text_version: 3 });
    const { calls } = await show(issue({ fields: [] }), {
      'PATCH /issues/501': failure(409, 'conflict'),
      'GET /issues/501': { body: quiet },
    });
    await page.getByRole('button', { name: 'Edit', exact: true }).click();
    await userEvent.type(summaryBox(), '!');
    await page.getByRole('button', { name: 'Save' }).click();

    await expect
      .element(page.getByRole('dialog', { name: 'Someone changed this issue' }))
      .toBeVisible();
    expect(sent(calls, 'PATCH').map(([, , body]) => body)).toEqual([
      { summary: 'Login button does nothing!', text_version: 2 },
      { summary: 'Login button does nothing!', text_version: 2 },
    ]);
  });

  it('«Keep editing» stays in the form with the saved text above it', async () => {
    const { dialog, screen } = await conflict();

    await dialog.getByRole('button', { name: 'Keep editing' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    await expect.element(summaryBox()).toHaveValue('Login button needs a handler');
    const above = screen.getByRole('group', { name: /^Saved meanwhile/ });
    await expect.element(above.getByRole('deletion')).toHaveTextContent('removed: is broken');
  });

  it('«Discard my edits» leaves edit mode with the saved text', async () => {
    const { dialog, screen } = await conflict();

    await dialog.getByRole('button', { name: 'Discard my edits' }).click();
    await expect.element(summaryBox()).not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Login button is broken' }))
      .toBeVisible();
  });
});

describe('ISS-034, ISS-035: tags', () => {
  it('adds a usable tag, creates a new one, removes one; tags link to their issues', async () => {
    const triage = tag(31, 'triage');
    const { screen, calls } = await show(issue({ fields: [] }), {
      'GET /tags': {
        body: { items: [triage, tag(32, 'read-only', false)], next_cursor: null },
      },
      'POST /tags': ({ body }) => ({ status: 201, body: tag(33, (body as { name: string }).name) }),
      'PUT /issues/501/tags/31': {
        body: issue({ fields: [], tags: [{ id: 31, name: 'triage', color: 4 }] }),
      },
      'PUT /issues/501/tags/33': {
        body: issue({
          fields: [],
          tags: [
            { id: 31, name: 'triage', color: 4 },
            { id: 33, name: 'urgent', color: 4 },
          ],
        }),
      },
      'DELETE /issues/501/tags/31': {
        body: issue({ fields: [], tags: [{ id: 33, name: 'urgent', color: 4 }] }),
      },
    });

    if (isPhone()) {
      await screen.getByRole('button', { name: 'More' }).click();
      await page.getByRole('menuitem', { name: 'Add tag' }).click();
    } else {
      await screen.getByRole('button', { name: 'Add tag' }).click();
    }
    const picker = page.getByRole('dialog', { name: 'Add tag' });
    await expect.element(picker.getByRole('checkbox', { name: 'triage' })).toBeVisible();
    // ISS-035: a tag shared only for viewing is not offered.
    expect(picker.getByRole('checkbox', { name: 'read-only' }).query()).toBeNull();
    await picker.getByRole('checkbox', { name: 'triage' }).click();
    await expect.element(picker.getByRole('checkbox', { name: 'triage' })).toBeChecked();

    await userEvent.type(picker.getByRole('searchbox'), 'urgent');
    await picker.getByRole('button', { name: 'Create tag “urgent”' }).click();
    await expect.element(picker.getByRole('checkbox', { name: 'urgent' })).toBeChecked();
    await userEvent.keyboard('{Escape}');

    const list = screen.getByRole('list', { name: 'Tags' });
    await expect.element(list.getByRole('link', { name: 'Issues tagged urgent' })).toBeVisible();
    expect(
      list.getByRole('link', { name: 'Issues tagged triage' }).element().getAttribute('href'),
    ).toMatch(/\/issues\?tag=31$/);
    if (isPhone()) expect(smallTargets(list.element())).toEqual([]);
    await list.getByRole('button', { name: 'Remove tag triage' }).click();
    await expect
      .element(list.getByRole('link', { name: 'Issues tagged triage' }))
      .not.toBeInTheDocument();

    expect(sent(calls)).toEqual([
      ['PUT', '/issues/501/tags/31', undefined],
      ['POST', '/tags', { name: 'urgent' }],
      ['PUT', '/issues/501/tags/33', undefined],
      ['DELETE', '/issues/501/tags/31', undefined],
    ]);
  });

  it('a reader sees tags without a remove button or «Add tag»', async () => {
    const { screen } = await show(
      issue({
        fields: [],
        tags: [{ id: 31, name: 'triage', color: null }],
        can: { ...issue().can, update: false },
      }),
    );
    await expect.element(screen.getByRole('link', { name: 'Issues tagged triage' })).toBeVisible();
    expect(screen.getByRole('button', { name: /Remove tag/ }).query()).toBeNull();
    expect(screen.getByRole('button', { name: 'Add tag' }).query()).toBeNull();
    expect(screen.getByRole('button', { name: 'Edit', exact: true }).query()).toBeNull();
  });
});

describe('ISS-030, ISS-032: star and vote', () => {
  it('toggle at once and roll back when refused', async () => {
    const { screen, calls } = await show(issue({ fields: [], starred: false, watchers: 2 }), {
      'PUT /issues/501/star': { status: 204 },
      'PUT /issues/501/vote': failure(403, 'forbidden'),
    });
    const details = screen.getByRole('group', { name: 'About the issue' });

    await details.getByRole('button', { name: /^Watch/, pressed: false }).click();
    await expect
      .element(details.getByRole('button', { name: /^Watch/, pressed: true }))
      .toBeVisible();
    await expect.element(details.getByText('3 watchers')).toBeInTheDocument();

    await details.getByRole('button', { name: /^Vote/ }).click();
    await expect.element(details.getByText('3 votes')).toBeInTheDocument();
    await expect
      .element(details.getByRole('button', { name: /^Vote/, pressed: false }))
      .toBeVisible();
    expect(sent(calls)).toEqual([
      ['PUT', '/issues/501/star', undefined],
      ['PUT', '/issues/501/vote', undefined],
    ]);
  });

  it('one’s own issue cannot be voted for, and the button says why', async () => {
    setSessionForTests({ user: { ...ANNA } } as unknown as Session);
    const { screen, calls } = await show(
      issue({ fields: [], can: { ...issue().can, vote: false } }),
    );
    const vote = screen.getByRole('button', { name: /^Vote/ });

    await expect.element(vote).toHaveAttribute('aria-disabled', 'true');
    await expect.element(vote).toHaveAccessibleDescription('You cannot vote for your own issue.');
    (vote.element() as HTMLElement).click();
    expect(sent(calls)).toEqual([]);
  });
});

describe('ISS-020: moving to another project', () => {
  const SITE = { ...WEB, id: 2, key: 'SITE', name: 'Site' };

  function routes(visible: boolean): Record<string, FakeRoute> {
    return {
      'GET /projects': {
        body: {
          items: [
            { ...WEB, archived_at: null },
            { ...SITE, archived_at: null },
            { ...WEB, id: 3, key: 'OLD', name: 'Old', archived_at: '2026-01-01T00:00:00Z' },
          ],
          next_cursor: null,
        },
      },
      'GET /issues/501/move-preview': {
        body: {
          target: SITE,
          loses_access: !visible,
          changes: [
            {
              field: { id: 11, name: 'Priority', kind: 'enum', multi: false, builtin: null },
              outcome: 'erased',
              before: {
                values: [{ id: 101, name: 'Major', color: 3, deleted: false }],
                users: [],
                groups: [],
                texts: [],
                numbers: [],
                files: [],
                links: [],
                tags: [],
                place: null,
              },
              after: {
                values: [],
                users: [],
                groups: [],
                texts: [],
                numbers: [],
                files: [],
                links: [],
                tags: [],
                place: null,
              },
            },
          ],
        },
      },
      'POST /issues/501/move': {
        body: {
          issue: visible ? issue({ fields: [], project: SITE, readable_id: 'SITE-4' }) : null,
          messages: [],
        },
      },
    };
  }

  it('previews the changes, moves and follows the issue to its new ID', async () => {
    const { screen, calls } = await show(issue({ fields: [] }), routes(true));

    await screen.getByRole('button', { name: 'Project: Web. Move to another' }).click();
    const dialog = page.getByRole('dialog', { name: 'Move to another project' });
    // Only active projects other than the issue's own.
    await expect.element(dialog.getByRole('radio', { name: 'Site' })).toBeVisible();
    expect(dialog.getByRole('radio', { name: 'Web' }).query()).toBeNull();
    expect(dialog.getByRole('radio', { name: 'Old' }).query()).toBeNull();
    await dialog.getByRole('radio', { name: 'Site' }).click();

    await expect.element(dialog.getByText('In Site these fields change:')).toBeVisible();
    await expect
      .element(dialog.getByRole('listitem'))
      .toHaveTextContent('Priority will be cleared: Major → empty');
    expect(dialog.getByText('You will not see the issue in that project.').query()).toBeNull();
    await dialog.getByRole('button', { name: 'Move' }).click();

    await expect.element(dialog).not.toBeInTheDocument();
    expect(sent(calls)).toEqual([['POST', '/issues/501/move', { project: 2 }]]);
    expect(navigation.goto).toHaveBeenCalledWith(
      expect.stringMatching(/\/issues\/SITE-4$/),
      expect.objectContaining({ replaceState: true }),
    );
    expect(toasts.items.map((toast) => toast.message)).toEqual(['Moved, now SITE-4']);
  });

  it('warns when the viewer would lose the issue and then leaves for the projects', async () => {
    const { screen } = await show(issue({ fields: [] }), routes(false));

    await screen.getByRole('button', { name: 'Project: Web. Move to another' }).click();
    const dialog = page.getByRole('dialog', { name: 'Move to another project' });
    await dialog.getByRole('radio', { name: 'Site' }).click();
    await expect
      .element(dialog.getByText('You will not see the issue in that project.'))
      .toBeVisible();
    await dialog.getByRole('button', { name: 'Move' }).click();

    await vi.waitFor(() => {
      expect(navigation.goto).toHaveBeenCalledWith(expect.stringMatching(/\/projects$/));
    });
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      'Moved. You no longer see the issue.',
    ]);
    expect(store.entities.table('issue').get(ID)).toBeUndefined();
  });
});

describe('ISS-028: delete', () => {
  it('asks naming the issue, deletes and opens the project’s issues', async () => {
    const { screen, calls } = await show(
      issue({ fields: [], can: { ...issue().can, delete: true } }),
      { 'DELETE /issues/501': { status: 204 } },
    );

    await screen.getByRole('button', { name: 'More' }).click();
    await page.getByRole('menuitem', { name: 'Delete' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete WEB-12?' });
    await expect.element(dialog.getByText(/“Login button does nothing” is deleted/)).toBeVisible();
    await dialog.getByRole('button', { name: 'Delete issue' }).click();

    await expect.element(dialog).not.toBeInTheDocument();
    expect(sent(calls)).toEqual([['DELETE', '/issues/501', undefined]]);
    expect(navigation.goto).toHaveBeenCalledWith(
      expect.stringMatching(/\/issues\?project=WEB$/),
      expect.objectContaining({ replaceState: true }),
    );
    expect(toasts.items.map((toast) => toast.message)).toEqual(['WEB-12 deleted']);
  });

  it('is not in the menu without the right', async () => {
    const { screen } = await show(issue({ fields: [] }));
    await screen.getByRole('button', { name: 'More' }).click();
    await expect.element(page.getByRole('menuitem', { name: 'Copy ID' })).toBeVisible();
    expect(page.getByRole('menuitem', { name: 'Delete' }).query()).toBeNull();
  });
});

describe('UI-006: copy actions', () => {
  it('copy the ID, the link and the ID with the summary', async () => {
    const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    const { screen } = await show(issue({ fields: [] }));

    for (const item of ['Copy ID', 'Copy link', 'Copy ID and summary']) {
      await screen.getByRole('button', { name: 'More' }).click();
      await page.getByRole('menuitem', { name: item }).click();
    }
    expect(writeText.mock.calls.map(([text]) => text)).toEqual([
      'WEB-12',
      expect.stringMatching(/^http.*\/issues\/WEB-12$/),
      'WEB-12 Login button does nothing',
    ]);
  });
});

describe('AUT-066: клонирование с задачи', () => {
  const COPY = issue({
    id: 502,
    readable_id: 'WEB-13',
    number: 13,
    summary: 'Login button does nothing (copy)',
    fields: [],
  });

  /** The project's rules, with cloning on or off. */
  function rules(on: boolean): FakeRoute {
    return {
      body: {
        version: 2,
        items: [{ rule: 'clone_issue', name: 'Issue cloning', enabled: on, needs_setup: [] }],
      },
    };
  }

  it('«Ещё» offers both clones when the project allows it; the toast opens the copy', async () => {
    const { screen, calls } = await show(issue({ fields: [] }), {
      'GET /projects/1/rules': rules(true),
      'POST /issues/501/clone': {
        status: 201,
        body: {
          issue: COPY,
          messages: [{ rule: 'clone_issue', code: 'clone_created', params: { issue: 'WEB-13' } }],
        },
      },
      'POST /issues/clone-drafts': {
        body: {
          drafts: [issue({ id: 601, draft: true, readable_id: null, number: null, fields: [] })],
          messages: [{ rule: 'clone_issue', code: 'draft_saved', params: { issue: 'WEB-12' } }],
        },
      },
    });

    await screen.getByRole('button', { name: 'More' }).click();
    await expect.element(page.getByRole('menuitem', { name: 'Clone' })).toBeVisible();
    await expect.element(page.getByRole('menuitem', { name: 'Clone as draft' })).toBeVisible();
    await page.getByRole('menuitem', { name: 'Clone' }).click();

    expect(sent(calls)).toEqual([['POST', '/issues/501/clone', undefined]]);
    expect(toasts.items.map((toast) => toast.message)).toEqual(['Copy created: WEB-13']);
    toasts.items[0]?.action?.run();
    expect(navigation.goto).toHaveBeenCalledWith('/issues/WEB-13');

    await screen.getByRole('button', { name: 'More' }).click();
    await page.getByRole('menuitem', { name: 'Clone as draft' }).click();
    expect(sent(calls).at(-1)).toEqual(['POST', '/issues/clone-drafts', { issues: [501] }]);
    expect(toasts.items.map((toast) => toast.message)).toEqual(['Draft copied from WEB-12']);
  });

  it('no cloning in the menu when the rule is off, the issue is a draft or there is no right', async () => {
    // Without the create right.
    setAccessForTests({
      global: [],
      projects: [{ project_id: 1, permissions: ['project.basic'] }],
      capabilities: [],
    });
    const noRight = await show(issue({ fields: [] }), {
      'GET /projects/1/rules': rules(true),
    });
    await noRight.screen.getByRole('button', { name: 'More' }).click();
    await expect.element(page.getByRole('menuitem', { name: 'Copy ID' })).toBeVisible();
    expect(page.getByRole('menuitem', { name: 'Clone' }).query()).toBeNull();
    await noRight.screen.unmount();

    // A draft is not cloneable (§3.10); the menu itself is the delete-only one.
    setAccessForTests(MEMBER);
    fakeApi({ 'GET /projects/1/rules': rules(true) });
    const draft = await render(IssueActions, {
      issue: issue({
        fields: [],
        draft: true,
        readable_id: null,
        number: null,
        can: { ...issue().can, delete: true },
      }),
      ondelete: () => undefined,
    });
    await draft.getByRole('button', { name: 'More' }).click();
    await expect.element(page.getByRole('menuitem', { name: 'Delete' })).toBeVisible();
    expect(page.getByRole('menuitem', { name: 'Clone' }).query()).toBeNull();
    await draft.unmount();

    // The rule off in the project.
    resetCloning();
    const off = await show(issue({ fields: [] }), { 'GET /projects/1/rules': rules(false) });
    await off.screen.getByRole('button', { name: 'More' }).click();
    expect(page.getByRole('menuitem', { name: 'Clone' }).query()).toBeNull();
  });
});
