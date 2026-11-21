// A project's overview and its frame on a desktop and on a phone (step 1.2; PRJ-005, PRJ-008,
// PRJ-011, PRJ-012, PRJ-014, PRJ-016, PRJ-061, PRJ-063).
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Project, Session, UserRef } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import ProjectFrame from '../ProjectFrame.svelte';
import { openProject, projectRow } from '../projects.ts';
import ProjectOverview from './ProjectOverview.svelte';

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

const BORIS: UserRef = {
  id: 2,
  full_name: 'Boris Lead',
  username: 'boris',
  avatar_color: 2,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

/** Administers project 1. */
const LEAD: AccessMap = {
  global: [],
  projects: [
    {
      project_id: 1,
      permissions: ['project.basic', 'project.full', 'project.update', 'project.delete'],
    },
  ],
  capabilities: [],
};

/** Sees project 1, nothing more. */
const READER: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic'] }],
  capabilities: [],
};

const WEB: Project = {
  id: 1,
  name: 'Web',
  key: 'WEB',
  archived_at: null,
  created_at: '2026-03-14T09:00:00Z',
  default_audience: { id: 7, name: 'Developers', builtin: null, team_of: null },
  deletion: null,
  description: '## Pages\n\n- [ ] Home\n- [x] Admin\n\n```rust\nfn main() {}\n```',
  description_html: html(false),
  favorite: false,
  former_keys: ['SITE', 'WWW'],
  kind: 'standard',
  logo_color: 2,
  logo_url: null,
  next_number: 1,
  owner: BORIS,
  team: { id: 101, name: 'Web', builtin: null, team_of: 1 },
  team_size: 4,
  template: 'scrum',
  version: 3,
};

/** The description as the server renders it, with the first box ticked or not. */
function html(home: boolean): string {
  return [
    '<h2>Pages</h2>',
    '<ul>',
    `<li data-sourcepos="3:1-3:10"><input type="checkbox"${home ? ' checked=""' : ''} disabled=""> Home</li>`,
    '<li data-sourcepos="4:1-5:0"><input type="checkbox" checked="" disabled=""> Admin</li>',
    '</ul>',
    '<pre><code class="language-rust">fn main() {}',
    '</code></pre>',
    '',
  ].join('\n');
}

function server(access: AccessMap, routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi({ 'GET /users/me/access': { body: access }, ...routes });
}

/** Loads the project the way the route does and returns its stored row. */
async function load(project: Project): Promise<Project> {
  const opened = await openProject(project.key);
  if (opened.kind !== 'found') throw new Error(`project ${project.key} is ${opened.kind}`);
  const row = projectRow(project.id);
  if (!row) throw new Error('no row');
  return row;
}

afterEach(() => {
  vi.unstubAllGlobals();
  store.entities.table('project').clear();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('ProjectOverview', () => {
  it('PRJ-014, PRJ-008, PRJ-005: description, owner, creation date, team, former keys and settings', async () => {
    server(LEAD, { 'GET /projects/by-key/WEB': { body: WEB } });
    const screen = await render(ProjectOverview, { project: await load(WEB), ondeleted: vi.fn() });

    await expect.element(screen.getByRole('heading', { level: 1, name: 'Web' })).toBeVisible();
    expect(screen.getByRole('heading', { level: 1 }).elements()).toHaveLength(1);
    // PRJ-006: the description is rendered Markdown, its headings under the section's.
    const description = screen.getByRole('region', { name: 'Description' });
    await expect
      .element(description.getByRole('heading', { level: 4, name: 'Pages' }))
      .toBeVisible();
    await expect.element(description.getByRole('checkbox', { name: 'Home' })).toBeEnabled();
    await expect.element(description.getByRole('region', { name: 'Code, rust' })).toBeVisible();
    const details = screen.getByRole('region', { name: 'Details' });
    await expect.element(details.getByText('Boris Lead')).toBeVisible();
    await expect.element(details.getByText('SITE, WWW')).toBeVisible();
    await expect.element(details.getByText('still lead here', { exact: false })).toBeVisible();
    expect(details.element().querySelector('time')?.getAttribute('datetime')).toContain(
      '2026-03-14',
    );
    await expect.element(details.getByText('4 people in the team')).toBeVisible();
    await expect.element(details.getByText('Scrum')).toBeVisible();
    await expect.element(details.getByText('Developers')).toBeVisible();
    expect(
      details.getByRole('link', { name: 'People and roles' }).element().getAttribute('href'),
    ).toMatch(/\/projects\/WEB\/settings\/people$/);

    const settings = screen.getByRole('region', { name: 'Settings' });
    expect(
      settings
        .getByRole('link')
        .elements()
        .map((link) => link.getAttribute('href')),
    ).toEqual([
      expect.stringMatching(/\/projects\/WEB\/settings\/general$/),
      expect.stringMatching(/\/projects\/WEB\/settings\/people$/),
      expect.stringMatching(/\/projects\/WEB\/settings\/fields$/),
    ]);
    await expect.element(screen.getByRole('link', { name: 'Settings' })).toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('PRJ-014: a reader sees the project, with no settings, people or actions', async () => {
    server(READER, {
      'GET /projects/by-key/WEB': {
        body: { ...WEB, description: '', former_keys: [], default_audience: null },
      },
    });
    const screen = await render(ProjectOverview, { project: await load(WEB), ondeleted: vi.fn() });

    await expect.element(screen.getByText('No description yet.')).toBeVisible();
    expect(screen.getByRole('link').query()).toBeNull();
    expect(screen.getByRole('button').query()).toBeNull();
    expect(screen.getByRole('region', { name: 'Settings' }).query()).toBeNull();
    expect(screen.getByText('Former keys').query()).toBeNull();
    await expectAccessible(screen.container);
  });

  it('PRJ-006, CNT-041: readers see the checklist but cannot tick it', async () => {
    server(READER, { 'GET /projects/by-key/WEB': { body: WEB } });
    const screen = await render(ProjectOverview, { project: await load(WEB), ondeleted: vi.fn() });

    await expect.element(screen.getByRole('checkbox', { name: 'Home' })).toBeDisabled();
    await expect.element(screen.getByRole('checkbox', { name: 'Admin' })).toBeChecked();
  });

  it('PRJ-006, CNT-041: ticking a box of the description saves it on the version shown', async () => {
    let current = WEB;
    const calls = server(LEAD, {
      'GET /projects/by-key/WEB': () => ({ body: current }),
      'PATCH /projects/1': ({ body }) => {
        const { description } = body as { description: string };
        current = { ...current, description, description_html: html(true), version: 4 };
        return { body: current };
      },
    });
    const screen = await render(ProjectOverview, { project: await load(WEB), ondeleted: vi.fn() });

    const home = screen.getByRole('checkbox', { name: 'Home' });
    await home.click();
    await vi.waitFor(() => {
      expect(projectRow(1)?.version).toBe(4);
    });
    expect(calls.find((call) => call.method === 'PATCH')?.body).toEqual({
      description: WEB.description.replace('- [ ] Home', '- [x] Home'),
      version: 3,
    });
    await expect.element(screen.getByRole('checkbox', { name: 'Home' })).toBeChecked();
    await expect.element(screen.getByRole('checkbox', { name: 'Admin' })).toBeChecked();
    await expectAccessible(screen.container);
  });

  it('PRJ-006, CNT-041: a clash with someone else’s change shows their text and says so', async () => {
    let current = WEB;
    server(LEAD, {
      'GET /projects/by-key/WEB': () => ({ body: current }),
      'GET /projects/1': () => ({ body: current }),
      'PATCH /projects/1': () => {
        current = {
          ...current,
          description: '- [ ] Home',
          description_html:
            '<ul>\n<li data-sourcepos="1:1-1:10"><input type="checkbox" disabled=""> Home</li>\n</ul>\n',
          version: 5,
        };
        return failure(409, 'conflict');
      },
    });
    const screen = await render(ProjectOverview, { project: await load(WEB), ondeleted: vi.fn() });

    await screen.getByRole('checkbox', { name: 'Admin' }).click();
    await vi.waitFor(() => {
      expect(toasts.items.map((toast) => toast.message)).toContain(
        'Someone changed the description at the same time. It shows their version now.',
      );
    });
    expect(screen.getByRole('checkbox', { name: 'Admin' }).query()).toBeNull();
    await expect.element(screen.getByRole('checkbox', { name: 'Home' })).not.toBeChecked();
  });

  it('PRJ-011, PRJ-061, PRJ-063: the header archives the project and restores it', async () => {
    const calls = server(LEAD, {
      'GET /projects/by-key/WEB': { body: WEB },
      'POST /projects/1/archive': {
        body: { ...WEB, archived_at: '2026-10-22T10:00:00Z', version: 4 },
      },
      'POST /projects/1/restore': { body: { ...WEB, version: 5 } },
    });
    const screen = await render(ProjectOverview, { project: await load(WEB), ondeleted: vi.fn() });

    await screen.getByRole('button', { name: 'Actions for “Web”' }).click();
    const menu = screen.getByRole('menu', { name: 'Actions for “Web”' });
    expect(
      menu
        .getByRole('menuitem')
        .elements()
        .map((item) => item.textContent.trim()),
    ).toEqual(['Archive project', 'Delete project']);
    await menu.getByRole('menuitem', { name: 'Archive project' }).click();
    const dialog = page.getByRole('dialog', { name: 'Archive “Web”?' });
    await dialog.getByRole('button', { name: 'Archive project' }).click();
    await expect.element(dialog).not.toBeInTheDocument();

    // Archived: restore in the header, no settings link and no archiving.
    const restore = screen.getByRole('button', { name: 'Restore project' });
    await expect.element(restore).toBeVisible();
    expect(screen.getByRole('link', { name: 'Settings', exact: true }).query()).toBeNull();
    await screen.getByRole('button', { name: 'Actions for “Web”' }).click();
    expect(
      screen
        .getByRole('menu', { name: 'Actions for “Web”' })
        .getByRole('menuitem')
        .elements()
        .map((item) => item.textContent.trim()),
    ).toEqual(['Delete project']);
    await page.getByRole('menuitem', { name: 'Delete project' }).click();
    await page
      .getByRole('dialog', { name: 'Delete “Web”?' })
      .getByRole('button', { name: 'Cancel' })
      .click();

    await restore.click();
    await expect.element(screen.getByRole('link', { name: 'Settings', exact: true })).toBeVisible();
    expect(calls.filter((call) => call.method === 'POST').map((call) => call.path)).toEqual([
      '/projects/1/archive',
      '/projects/1/restore',
    ]);
  });
});

describe('ProjectFrame', () => {
  const children = createRawSnippet(() => ({ render: () => '<p>Page content</p>' }));

  it('PRJ-016, PRJ-012: the project’s sections, the current one marked, and the star', async () => {
    const calls = server(LEAD, {
      'GET /projects/by-key/WEB': { body: WEB },
      'PUT /projects/1/favorite': { status: 204 },
    });
    const project = await load(WEB);
    const screen = await render(ProjectFrame, {
      project,
      pathname: '/projects/WEB',
      children,
    });

    const nav = screen.getByRole('navigation', { name: 'Project' });
    expect(
      nav
        .getByRole('link')
        .elements()
        .map((link) => link.textContent.trim()),
    ).toEqual(['Overview', 'Issues', 'Boards', 'Settings']);
    await expect
      .element(nav.getByRole('link', { name: 'Overview' }))
      .toHaveAttribute('aria-current', 'page');
    await expect.element(screen.getByRole('link', { name: 'Web' })).toBeVisible();
    await expect.element(screen.getByText('Page content')).toBeVisible();
    // The frame names the project as a link; the page below has the h1.
    expect(screen.getByRole('heading').query()).toBeNull();

    await screen.getByRole('button', { name: 'Add to favorites' }).click();
    await expect
      .element(screen.getByRole('button', { name: 'Remove from favorites' }))
      .toHaveAttribute('aria-pressed', 'true');
    expect(calls.some((call) => call.method === 'PUT')).toBe(true);
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('PRJ-016: settings only for people who configure the project; archived projects say so', async () => {
    server(READER, {
      'GET /projects/by-key/WEB': { body: { ...WEB, archived_at: '2026-10-21T09:00:00Z' } },
    });
    const project = await load(WEB);
    const screen = await render(ProjectFrame, {
      project,
      pathname: '/projects/WEB/settings/general',
      children,
    });

    const nav = screen.getByRole('navigation', { name: 'Project' });
    expect(nav.getByRole('link', { name: 'Settings' }).query()).toBeNull();
    await expect.element(screen.getByText('Archived', { exact: true })).toBeVisible();
    await expect
      .element(screen.getByText('This project is archived', { exact: false }))
      .toBeVisible();
    await expectAccessible(screen.container);
  });
});
