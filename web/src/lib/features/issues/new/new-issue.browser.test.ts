// The create menu, the new-issue form and its draft on a desktop and on a phone (step 1.7a-5;
// ISS-005…ISS-008, ISS-010, ISS-011, UI-002).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Issue, Project, Tag } from '#lib/api/schema.gen.ts';
import { preferences, setAccessForTests, setPreferencesForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';
import Toaster from '#lib/ui/Toaster.svelte';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { xhrThroughFetch } from '../../../../test/fake-xhr.ts';
import { ANNA, ASSIGNEE, issue, PRIORITY } from '../test-data.ts';
import CreateMenu from './CreateMenu.svelte';
import { forgetLinkTypes } from '../links/links.svelte.ts';
import { DraftSession } from './drafts.svelte.ts';
import NewIssueHost from './NewIssueHost.svelte';
import NewIssuePage from './NewIssuePage.svelte';
import { newIssueDialog, openNewIssue } from './open.svelte.ts';

const navigation = vi.hoisted(() => ({
  goto: vi.fn<(href: string, options?: unknown) => Promise<void>>(() => Promise.resolve()),
}));
vi.mock('$app/navigation', () => navigation);

const CREATOR: AccessMap = {
  global: [],
  projects: [1, 2, 3].map((project_id) => ({
    project_id,
    permissions: ['project.basic', 'issue.create', 'issue.update', 'attachment.create'],
  })),
  capabilities: [],
};

function project(id: number, name: string, key: string, extra: Partial<Project> = {}): Project {
  return {
    id,
    name,
    key,
    archived_at: null,
    created_at: '2026-10-20T09:00:00Z',
    default_audience: null,
    deletion: null,
    description: '',
    description_html: '',
    favorite: false,
    former_keys: [],
    kind: 'standard',
    logo_color: id % 8,
    logo_url: null,
    next_number: 1,
    owner: ANNA,
    team: { id: 100 + id, name, builtin: null, team_of: id },
    team_size: 1,
    template: 'blank',
    version: 1,
    ...extra,
  };
}

const API = project(1, 'Api', 'API');
const WEB_P = project(2, 'Web', 'WEB', { favorite: true });
const OPS = project(3, 'Ops', 'OPS');
const ref = (p: Project) => ({
  id: p.id,
  key: p.key,
  name: p.name,
  logo_color: p.logo_color,
  logo_url: null,
  archived: false,
});

const DRAFT = 900;

function draft(rest: Partial<Issue> = {}): Issue {
  return issue({
    id: DRAFT,
    readable_id: null,
    number: null,
    draft: true,
    project: ref(WEB_P),
    summary: '',
    description: '',
    description_html: '',
    fields: [PRIORITY, { ...ASSIGNEE, users: [] }],
    tags: [],
    starred: false,
    votes: 0,
    text_version: 1,
    ...rest,
  });
}

function tag(id: number, name: string): Tag {
  return {
    id,
    name,
    color: 4,
    owner: ANNA,
    sharing: null,
    abilities: { apply: true, edit: false, delete: false, share: false, transfer: false },
    version: 1,
    created_at: '2026-10-01T00:00:00Z',
  };
}

/** A server with three projects and one draft. */
function server(routes: Record<string, FakeRoute> = {}): FakeRequest[] {
  let current = draft();
  return fakeApi({
    'GET /projects': { body: { items: [OPS, WEB_P, API], next_cursor: null } },
    'POST /issues': ({ body }) => {
      const sent = body as { project: number; summary: string; description: string };
      const chosen = [API, WEB_P, OPS].find((p) => p.id === sent.project) ?? WEB_P;
      current = draft({
        project: ref(chosen),
        summary: sent.summary,
        description: sent.description,
      });
      return { status: 201, body: { issue: current, messages: [] } };
    },
    'PATCH /issues/900': ({ body }) => {
      current = { ...current, ...(body as Partial<Issue>), text_version: current.text_version + 1 };
      return { body: { issue: current, messages: [] } };
    },
    'DELETE /issues/900': { status: 204 },
    'GET /issues/drafts': { body: { items: [], next_cursor: null } },
    'GET /issues/900/links': { body: { groups: [], can_link: true } },
    'POST /issues/900/publish': () => ({
      body: {
        issue: { ...current, draft: false, readable_id: 'WEB-12', number: 12 },
        messages: [],
      },
    }),
    ...routes,
  });
}

function sent(calls: FakeRequest[]) {
  return calls
    .filter((call) => call.method !== 'GET')
    .map((call) => [call.method, call.path, call.body]);
}

const summaryBox = () => page.getByRole('textbox', { name: /^Summary/ });
const createButton = () => page.getByRole('button', { name: 'Create', exact: true });
async function openFieldsOnPhone(): Promise<void> {
  if (isPhone()) await page.getByText('Fields', { exact: true }).click();
}

async function chooseAction(name: string): Promise<void> {
  if (isPhone()) {
    await page.getByRole('button', { name: 'More', exact: true }).click();
    const item = name === 'Watch it' ? 'menuitemradio' : 'menuitem';
    await page.getByRole(item, { name, exact: true }).click();
  } else {
    await page.getByRole('button', { name, exact: true }).click();
  }
}

beforeEach(() => {
  setAccessForTests(CREATOR);
  setPreferencesForTests({});
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  navigation.goto.mockClear();
  newIssueDialog.open = false;
  store.entities.table('issue').clear();
  toasts.clear();
  forgetLinkTypes();
  setAccessForTests(null);
  setPreferencesForTests(null);
});

describe('ISS-005: a new issue', () => {
  it('starts a draft in the given project, focused on the summary; Create waits for a summary', async () => {
    const calls = server();
    await render(NewIssuePage, { project: 'OPS' });

    await expect.element(summaryBox()).toHaveFocus();
    expect(sent(calls)[0]).toEqual([
      'POST',
      '/issues',
      { project: 3, draft: true, summary: '', description: '' },
    ]);
    await openFieldsOnPhone();
    await expect.element(page.getByRole('button', { name: 'Ops', exact: true })).toBeVisible();
    await expect.element(createButton()).toBeDisabled();
    await userEvent.type(summaryBox(), '   ');
    await expect.element(createButton()).toBeDisabled();

    const form = page.getByRole('form', { name: 'New issue form' });
    await expectAccessible(form.element());
    if (isPhone()) expect(await smallTargets(form.element())).toEqual([]);

    await page.getByRole('button', { name: 'Visible to everyone who sees the project' }).click();
    const visibility = page.getByRole('dialog', { name: 'Visibility' });
    await expect
      .element(visibility.getByRole('radio', { name: 'Everyone who sees the project' }))
      .toBeChecked();
  });

  it('takes the last project without a given one and lists the context, favorites, then names', async () => {
    setPreferencesForTests({ last_project: 1 });
    const calls = server();
    await render(NewIssuePage, {});

    await expect.element(summaryBox()).toBeVisible();
    expect(sent(calls)[0]?.[2]).toMatchObject({ project: 1 });
    await openFieldsOnPhone();
    await page.getByRole('button', { name: 'Api', exact: true }).click();
    const radios = page.getByRole('group', { name: 'Project of the issue' }).getByRole('radio');
    await expect.poll(() => radios.elements().length).toBe(3);
    const names = radios
      .elements()
      .map((radio) => radio.closest('label')?.textContent.trim().split(/\s+/)[0]);
    expect(names).toEqual(['Web', 'Api', 'Ops']);
  });

  it('Ctrl+Enter creates the issue, remembers the project and opens it', async () => {
    const calls = server();
    await render(NewIssuePage, {});
    await render(Toaster);

    await userEvent.type(summaryBox(), 'Login fails');
    await userEvent.keyboard('{Control>}{Enter}{/Control}');

    await expect.poll(() => navigation.goto.mock.calls.at(-1)?.[0]).toBe('/issues/WEB-12');
    expect(sent(calls).slice(-2)).toEqual([
      ['PATCH', '/issues/900', { summary: 'Login fails', description: '', text_version: 1 }],
      ['POST', '/issues/900/publish', undefined],
    ]);
    expect(preferences.chosen('last_project')).toBe(2);
    await expect.element(page.getByText('Issue WEB-12 created')).toBeVisible();
  });
});

describe('ISS-006: values of a new issue', () => {
  it('shows the defaults and marks the required fields a refused create names', async () => {
    server({
      'POST /issues/900/publish': failure(400, 'invalid_request', [
        { path: 'fields.12', code: 'required', message: 'required' },
      ]),
    });
    await render(NewIssuePage, {});

    const panel = page.getByRole('complementary', { name: 'Fields' });
    await openFieldsOnPhone();
    await expect.element(panel.getByText('Major')).toBeVisible();
    await userEvent.type(summaryBox(), 'Login fails');
    await createButton().click();

    await expect.element(page.getByText('Fill in the required fields first.')).toBeVisible();
    await expect.element(panel.getByText('Fill this field in')).toBeVisible();
    await expect.element(panel.getByRole('button', { name: /Unassigned/ })).toHaveFocus();
    expect(navigation.goto.mock.calls.map(([href]) => href)).not.toContain('/issues/WEB-12');
  });
});

describe('ISS-007: tags, star and files of a new issue', () => {
  it('puts a tag on the draft and stars it at once', async () => {
    const calls = server({
      'GET /tags': { body: { items: [tag(7, 'frontend')], next_cursor: null } },
      'PUT /issues/900/tags/7': { status: 204 },
      'PUT /issues/900/star': { status: 204 },
    });
    await render(NewIssuePage, {});

    await expect.element(summaryBox()).toBeVisible();
    await chooseAction('Add tag');
    await page.getByRole('checkbox', { name: 'frontend' }).click();
    await userEvent.keyboard('{Escape}');
    await chooseAction('Watch it');

    if (isPhone()) {
      await page.getByRole('button', { name: 'More', exact: true }).click();
      await expect
        .element(page.getByRole('menuitemradio', { name: 'Watch it' }))
        .toHaveAttribute('aria-checked', 'true');
    } else {
      await expect
        .element(page.getByRole('button', { name: 'Watch it' }))
        .toHaveAttribute('aria-pressed', 'true');
    }
    expect(sent(calls).slice(1)).toEqual([
      ['PUT', '/issues/900/tags/7', undefined],
      ['PUT', '/issues/900/star', undefined],
    ]);
  });

  it('attaches a file from an editor to the draft, so the draft is not empty', async () => {
    const calls = server({
      'POST /uploads': {
        status: 201,
        body: {
          id: 40,
          name: 'shot.png',
          kind: 'image',
          size: 3,
          content_type: 'image/png',
          created_at: '2026-10-24T00:00:00Z',
          url: '/api/v1/uploads/40/shot.png',
        },
      },
      'POST /issues/900/attachments': {
        status: 201,
        body: { attachments: [{ name: 'shot (2).png' }], next: null },
      },
    });
    xhrThroughFetch();
    const session = new DraftSession();
    await session.start({ project: 2 });
    expect(session.isEmpty()).toBe(true);

    const upload = await session.upload(new File(['png'], 'shot.png', { type: 'image/png' }));

    expect(upload.name).toBe('shot (2).png');
    expect(session.isEmpty()).toBe(false);
    expect(sent(calls).at(-1)).toEqual(['POST', '/issues/900/attachments', { uploads: [40] }]);
    session.release();
  });
});

describe('ISS-008: ways to create', () => {
  it('«Create and start another» publishes and opens a fresh form; the same text keeps the texts', async () => {
    const calls = server();
    await render(NewIssuePage, {});

    await userEvent.type(summaryBox(), 'First');
    await page.getByRole('button', { name: 'Other ways to create' }).click();
    await page.getByRole('menuitem', { name: 'Create and start another', exact: true }).click();
    await expect.element(summaryBox()).toHaveValue('');

    await userEvent.type(summaryBox(), 'Second');
    await page.getByRole('button', { name: 'Other ways to create' }).click();
    await page
      .getByRole('menuitem', { name: 'Create and start another with the same text' })
      .click();
    await expect.poll(() => sent(calls).filter(([method]) => method === 'POST').length).toBe(5);
    await expect.element(summaryBox()).toHaveValue('Second');

    const posts = sent(calls).filter(([method, path]) => method === 'POST' && path === '/issues');
    expect(posts.map((call) => (call[2] as { summary: string }).summary)).toEqual([
      '',
      '',
      'Second',
    ]);
    expect(navigation.goto.mock.calls.filter(([href]) => href.startsWith('/issues/WEB'))).toEqual(
      [],
    );
  });

  it('«Cancel and delete» deletes the draft and leaves', async () => {
    const calls = server();
    await render(NewIssuePage, {});

    await userEvent.type(summaryBox(), 'Never mind');
    await page.getByRole('button', { name: 'Cancel and delete' }).click();

    await expect.poll(() => navigation.goto.mock.calls.at(-1)?.[0]).toBe('/issues?project=WEB');
    expect(sent(calls).filter(([method]) => method === 'DELETE')).toEqual([
      ['DELETE', '/issues/900', undefined],
    ]);
  });
});

describe('ISS-010: the draft', () => {
  it('saves the texts a moment after typing stops', async () => {
    const calls = server();
    await render(NewIssuePage, {});

    await userEvent.type(summaryBox(), 'Crash on save');
    await expect.element(page.getByRole('status').getByText('Draft saved')).toBeVisible();
    expect(sent(calls).filter(([method]) => method === 'PATCH')).toEqual([
      ['PATCH', '/issues/900', { summary: 'Crash on save', description: '', text_version: 1 }],
    ]);
  });

  it('resends once on the newer text version after a conflict from another tab', async () => {
    let tries = 0;
    const calls = server({
      'PATCH /issues/900': ({ body }) => {
        tries += 1;
        const saved = draft({ ...(body as Partial<Issue>), text_version: 6 });
        return tries === 1 ? failure(409, 'conflict') : { body: { issue: saved, messages: [] } };
      },
      'GET /issues/900': { body: draft({ text_version: 5 }) },
    });
    const session = new DraftSession();
    await session.start({ project: 2 });
    session.summary = 'Mine';
    session.typed();
    await session.flush();

    expect(
      sent(calls)
        .filter(([method]) => method === 'PATCH')
        .map((call) => call[2]),
    ).toEqual([
      { summary: 'Mine', description: '', text_version: 1 },
      { summary: 'Mine', description: '', text_version: 5 },
    ]);
    session.release();
  });

  it.skipIf(isPhone())(
    'uses a compact dialog with a quiet focus and a growing description',
    async () => {
      server();
      await render(NewIssueHost);
      openNewIssue();
      const dialog = page.getByRole('dialog', { name: 'New issue' });
      await expect.element(summaryBox()).toHaveFocus();

      expect(dialog.element().querySelector('.form-title')).toBeNull();
      const summaryStyle = getComputedStyle(summaryBox().element());
      expect(summaryStyle.outlineStyle).toBe('none');
      expect(summaryStyle.borderBottomWidth).toBe('1px');

      const description = dialog.getByRole('textbox', { name: 'Description', exact: true });
      const editor = description.element().closest('.editor');
      const frame = description.element().closest('.frame');
      if (!editor || !frame) throw new Error('no description editor');
      expect(editor.classList.contains('grow')).toBe(true);
      expect(getComputedStyle(description.element()).flexGrow).toBe('1');
      expect(frame.getBoundingClientRect().height).toBeGreaterThan(280);
    },
  );

  it('closing the dialog deletes an empty draft and keeps one with text, with a toast', async () => {
    if (isPhone()) {
      openNewIssue({ project: 'WEB' });
      expect(navigation.goto).toHaveBeenCalledWith('/issues/new?project=WEB');
      return;
    }
    const calls = server();
    await render(NewIssueHost);
    await render(Toaster);

    openNewIssue();
    const dialog = page.getByRole('dialog', { name: 'New issue' });
    await expect.element(summaryBox()).toHaveFocus();
    await userEvent.keyboard('{Escape}');
    await expect.element(dialog).not.toBeInTheDocument();
    await expect
      .poll(() => sent(calls).filter(([method]) => method === 'DELETE'))
      .toEqual([['DELETE', '/issues/900', undefined]]);

    openNewIssue();
    await expect.element(summaryBox()).toHaveFocus();
    await userEvent.type(summaryBox(), 'Keep me');
    await userEvent.keyboard('{Escape}');
    await expect.element(page.getByText('Draft saved')).toBeVisible();
    expect(sent(calls).filter(([method]) => method === 'DELETE')).toHaveLength(1);
    expect(
      sent(calls)
        .filter(([method]) => method === 'PATCH')
        .at(-1)?.[2],
    ).toMatchObject({
      summary: 'Keep me',
    });

    await page.getByRole('button', { name: 'Open', exact: true }).click();
    await expect.element(dialog).toBeVisible();
    expect(newIssueDialog.target).toEqual({ draft: DRAFT });
    // The draft is gone on this fake server: an empty new one is started, and deleted on close.
    await expect.element(summaryBox()).toHaveFocus();
    await userEvent.keyboard('{Escape}');
    await expect.poll(() => sent(calls).filter(([method]) => method === 'DELETE')).toHaveLength(2);
  });
});

describe('ISS-011: the drafts menu', () => {
  it('lists the other drafts and deletes them all after asking', async () => {
    const calls = server({
      'GET /issues/drafts': {
        body: {
          items: [draft(), draft({ id: 901, summary: 'Old idea' }), draft({ id: 902 })],
          next_cursor: null,
        },
      },
      'DELETE /issues/drafts': { status: 204 },
    });
    await render(NewIssuePage, {});
    await render(Toaster);

    await expect.element(summaryBox()).toBeVisible();
    await page.getByRole('button', { name: 'Drafts' }).click();
    const menu = page.getByRole('menu', { name: 'Your drafts' });
    await expect.element(menu.getByRole('menuitem', { name: /^Old idea · / })).toBeVisible();
    await expect.element(menu.getByRole('menuitem', { name: /^No summary · / })).toBeVisible();
    await menu.getByRole('menuitem', { name: 'Delete all drafts' }).click();
    const confirm = page.getByRole('dialog', { name: 'Delete all drafts?' });
    await confirm.getByRole('button', { name: 'Delete all drafts' }).click();

    await expect.element(page.getByText('Drafts deleted')).toBeVisible();
    expect(sent(calls).slice(1)).toEqual([
      ['DELETE', '/issues/drafts', undefined],
      ['POST', '/issues', { project: 2, draft: true, summary: '', description: '' }],
    ]);
  });
});

describe('UI-002: the create menu', () => {
  it('offers an issue to people who may create one, and nothing to others', async () => {
    const screen = await render(CreateMenu, {});
    await screen.getByRole('button', { name: 'New' }).click();
    await expect.element(page.getByRole('menuitem', { name: 'Issue' })).toBeVisible();
    await userEvent.keyboard('{Escape}');
    await screen.unmount();

    setAccessForTests({
      global: [],
      projects: [{ project_id: 1, permissions: ['project.basic'] }],
      capabilities: [],
    });
    const other = await render(CreateMenu, {});
    expect(other.container.querySelector('button')).toBeNull();
  });
});

describe('LNK-007, ISS-009: links of a new issue', () => {
  const SUBTASK = {
    id: 4,
    name: 'Subtask',
    source_name: 'parent of',
    target_name: 'child of',
    direction: 'hierarchy',
    builtin: 'subtask',
    locked: true,
    deleted_at: null,
    usage: null,
    version: 1,
  };
  const PARENT = issue({ id: 501, readable_id: 'WEB-12', summary: 'Login epic' });
  const parentGroup = {
    link_type: SUBTASK,
    side: 'target',
    name: 'child of',
    items: [
      {
        link: 81,
        issue: {
          id: 501,
          readable_id: 'WEB-12',
          summary: 'Login epic',
          project: PARENT.project,
          resolved: false,
          restricted: false,
        },
        linked_at: '2026-10-24T10:00:00Z',
      },
    ],
    total: 1,
    unresolved: 1,
    next_cursor: null,
  };

  it('a subtask starts with its parent; the draft with a link is kept on close', async () => {
    const calls = server({
      'GET /link-types': { body: { items: [SUBTASK] } },
      'GET /issues/900/links': { body: { groups: [parentGroup], can_link: true } },
      'POST /issues/batch': { body: { items: [PARENT] } },
    });
    const screen = await render(NewIssuePage, { project: 'WEB', parent: 501 });

    const links = screen.getByRole('region', { name: /^Links/ });
    await expect.element(links.getByRole('link', { name: /WEB-12/ })).toBeVisible();
    expect(sent(calls)[0]).toEqual([
      'POST',
      '/issues',
      {
        project: 2,
        draft: true,
        summary: '',
        description: '',
        links: [{ issue: 501, link_type: 4, side: 'target' }],
      },
    ]);

    await screen.unmount();
    await vi.waitFor(() => {
      expect(toasts.items.map((toast) => toast.message)).toContain('Draft saved');
    });
    expect(sent(calls).some(([method]) => method === 'DELETE')).toBe(false);
  });

  it('adds a link to the draft from the title action', async () => {
    const calls = server({
      'GET /link-types': { body: { items: [SUBTASK] } },
      'POST /issues/batch': { body: { items: [PARENT] } },
      // Linking refetches both ends.
      'GET /issues/900': { body: draft() },
      'POST /issues/900/links': {
        body: {
          added: [501],
          existing: [],
          links: { groups: [parentGroup], can_link: true },
          messages: [],
        },
      },
    });
    preferences.set({ recent_issues: [501] });
    const screen = await render(NewIssuePage, {});

    await chooseAction('Add a link');
    const dialog = page.getByRole('dialog', { name: 'Link issues' });
    await dialog.getByRole('combobox', { name: 'Link' }).selectOptions('child of');
    await dialog.getByRole('checkbox', { name: /WEB-12/ }).click();
    await dialog.getByRole('button', { name: 'Link', exact: true }).click();

    const links = screen.getByRole('region', { name: /^Links/ });
    await expect.element(links.getByRole('link', { name: /WEB-12/ })).toBeVisible();
    expect(sent(calls)).toContainEqual([
      'POST',
      '/issues/900/links',
      { link_type: 4, side: 'target', issues: [501] },
    ]);
  });
});
