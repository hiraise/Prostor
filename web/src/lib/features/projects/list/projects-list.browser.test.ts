// The projects list on a desktop and on a phone (step 1.2; PRJ-007, PRJ-010…PRJ-013, PRJ-061,
// PRJ-063…PRJ-065).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Project, Session, UserRef } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { ensureProjects, type ProjectSection } from '../projects.ts';
import ProjectsPage from './ProjectsPage.svelte';

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

/** May create, change and delete every project. */
const ADMIN: AccessMap = {
  global: ['project.create', 'project.basic', 'project.full', 'project.update', 'project.delete'],
  projects: [],
  capabilities: [],
};

/** Configures and may delete project 1 only; sees the others. */
const LEAD: AccessMap = {
  global: [],
  projects: [
    {
      project_id: 1,
      permissions: ['project.basic', 'project.full', 'project.update', 'project.delete'],
    },
    { project_id: 2, permissions: ['project.basic'] },
  ],
  capabilities: [],
};

/** Sees projects, changes none. */
const MEMBER: AccessMap = {
  global: [],
  projects: [
    { project_id: 1, permissions: ['project.basic'] },
    { project_id: 2, permissions: ['project.basic'] },
  ],
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

const WEB = project(1, 'Web', 'WEB', { team_size: 3 });
const MOBILE = project(2, 'Mobile', 'MOB', { favorite: true, team_size: 5 });
const DOCS = project(3, 'Docs', 'DOCS', { owner: null });
const OLD = project(4, 'Legacy', 'LEG', { archived_at: '2026-10-21T09:00:00Z' });

function pageOf(items: Project[], next: string | null = null): { body: unknown } {
  return { body: { items, next_cursor: next } };
}

/** Answers `GET /projects` by section and search text. */
function projectsRoute(
  answer: (section: ProjectSection, q: string, request: FakeRequest) => Project[],
): FakeRoute {
  return (request) => {
    const section = (request.query.get('section') ?? 'active') as ProjectSection;
    return pageOf(answer(section, request.query.get('q') ?? '', request));
  };
}

function server(access: AccessMap, routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi({ 'GET /users/me/access': { body: access }, ...routes });
}

/** A list of the store as the route loads it, fetched again (lists stay cached between tests). */
async function fresh(
  section: ProjectSection,
  text = '',
): Promise<Awaited<ReturnType<typeof ensureProjects>>> {
  const list = await ensureProjects(section, text);
  await list.refresh();
  return list;
}

async function listPage(
  options: {
    query?: string;
    deleting?: boolean;
    onnavigate?: () => unknown;
    onsearch?: () => unknown;
  } = {},
): Promise<Screen> {
  const query = options.query ?? '';
  const active = await fresh('active', query);
  const deleting = options.deleting ? await fresh('deleting', query) : null;
  return render(ProjectsPage, {
    active,
    deleting,
    query,
    onnavigate: options.onnavigate ?? vi.fn(),
    onsearch: options.onsearch,
  });
}

type Screen = Awaited<ReturnType<typeof render>>;

function rowNames(screen: Screen, list: string): string[] {
  return screen
    .getByRole('list', { name: list })
    .getByRole('link')
    .elements()
    .map((link) => link.textContent.trim());
}

afterEach(() => {
  vi.unstubAllGlobals();
  store.entities.table('project').clear();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
});

describe('ProjectsPage', () => {
  it('PRJ-010, PRJ-012, PRJ-007: rows show the logo, name, key, owner and team, favorites first', async () => {
    server(ADMIN, {
      'GET /projects': projectsRoute((section) =>
        section === 'active' ? [MOBILE, DOCS, WEB] : [],
      ),
    });
    const screen = await listPage();

    await expect.element(screen.getByRole('heading', { level: 1, name: 'Projects' })).toBeVisible();
    expect(rowNames(screen, 'Projects')).toEqual(['Mobile', 'Docs', 'Web']);
    const list = screen.getByRole('list', { name: 'Projects' });
    expect(list.getByRole('link', { name: 'Web' }).element().getAttribute('href')).toMatch(
      /\/projects\/WEB$/,
    );
    await expect.element(list.getByText('WEB', { exact: true })).toBeVisible();
    await expect.element(list.getByText('3 people in the team')).toBeVisible();
    await expect.element(list.getByText('1 person in the team').first()).toBeVisible();
    expect(list.getByText('Owner: Anna Admin').elements()).toHaveLength(2);
    // The generated logo: the key's first letters.
    const logos = [...list.element().querySelectorAll('.logo')].map((logo) => logo.textContent);
    expect(logos).toEqual(['MO', 'DO', 'WE']);
    await expect
      .element(screen.getByRole('button', { name: 'Remove “Mobile” from favorites' }))
      .toHaveAttribute('aria-pressed', 'true');
    await expect
      .element(screen.getByRole('button', { name: 'Add “Web” to favorites' }))
      .toHaveAttribute('aria-pressed', 'false');
    await expect.element(screen.getByRole('link', { name: 'Create project' })).toBeVisible();
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('PRJ-010: Russian plurals, and no creating without the permission', async () => {
    setLocale('ru');
    server(MEMBER, {
      'GET /projects': projectsRoute((section) => (section === 'active' ? [MOBILE, WEB] : [])),
    });
    const screen = await listPage();

    await expect.element(screen.getByText('5 человек в команде')).toBeVisible();
    await expect.element(screen.getByText('3 человека в команде')).toBeVisible();
    await expect.element(screen.getByText('Владелец: Anna Admin').first()).toBeVisible();
    expect(screen.getByRole('link', { name: 'Создать проект' }).query()).toBeNull();
    // Nobody here may delete a project: no "being deleted" section.
    expect(screen.getByRole('heading', { name: 'Удаляются' }).query()).toBeNull();
    await expectAccessible(screen.container);
  });

  it('PRJ-013: the first ten are shown, then the rest with “Show more”', async () => {
    const many = Array.from({ length: 12 }, (_, index) =>
      project(10 + index, `Project ${String(index + 1).padStart(2, '0')}`, `P${String(index + 1)}`),
    );
    const extra = project(40, 'Project 13', 'P13');
    const calls = server(ADMIN, {
      'GET /projects': (request) => {
        if (request.query.get('section') !== 'active') return pageOf([]);
        return request.query.get('cursor') === 'next' ? pageOf([extra]) : pageOf(many, 'next');
      },
    });
    const screen = await listPage();

    await expect.element(screen.getByRole('link', { name: 'Project 01' })).toBeVisible();
    expect(rowNames(screen, 'Projects')).toHaveLength(10);
    await screen.getByRole('button', { name: 'Show more' }).click();
    await vi.waitFor(() => {
      expect(rowNames(screen, 'Projects')).toHaveLength(12);
    });
    // Every loaded row is shown; the next page comes from the server.
    await screen.getByRole('button', { name: 'Show more' }).click();
    await expect.element(screen.getByRole('link', { name: 'Project 13' })).toBeVisible();
    expect(calls.some((call) => call.query.get('cursor') === 'next')).toBe(true);
    await expect.element(screen.getByRole('button', { name: 'Show more' })).not.toBeInTheDocument();
  });

  it('PRJ-012: the star adds and removes a favorite at once', async () => {
    const calls = server(MEMBER, {
      'GET /projects': projectsRoute((section) => (section === 'active' ? [MOBILE, WEB] : [])),
      'PUT /projects/1/favorite': { status: 204 },
      'DELETE /projects/2/favorite': failure(404, 'not_found'),
    });
    const screen = await listPage();

    await screen.getByRole('button', { name: 'Add “Web” to favorites' }).click();
    await expect
      .element(screen.getByRole('button', { name: 'Remove “Web” from favorites' }))
      .toHaveAttribute('aria-pressed', 'true');
    expect(calls.filter((call) => call.method === 'PUT').map((call) => call.path)).toEqual([
      '/projects/1/favorite',
    ]);

    // A refused change goes back.
    await screen.getByRole('button', { name: 'Remove “Mobile” from favorites' }).click();
    await expect
      .element(screen.getByRole('button', { name: 'Remove “Mobile” from favorites' }))
      .toHaveAttribute('aria-pressed', 'true');
    expect(calls.some((call) => call.method === 'DELETE')).toBe(true);
  });

  it('PRJ-011: the row menu offers what the person may do there', async () => {
    server(LEAD, {
      'GET /projects': projectsRoute((section) => (section === 'active' ? [MOBILE, WEB] : [])),
    });
    const onnavigate = vi.fn();
    const screen = await listPage({ onnavigate });

    await screen.getByRole('button', { name: 'Actions for “Web”' }).click();
    const web = screen.getByRole('menu', { name: 'Actions for “Web”' });
    await expect.element(web).toBeVisible();
    expect(
      web
        .getByRole('menuitem')
        .elements()
        .map((item) => item.textContent.trim()),
    ).toEqual(['Open project', 'Project settings', 'Archive project', 'Delete project']);
    expect(smallTargets(screen.container, isPhone() ? 44 : 28)).toEqual([]);
    await web.getByRole('menuitem', { name: 'Project settings' }).click();
    expect(onnavigate).toHaveBeenCalledWith(
      expect.stringMatching(/\/projects\/WEB\/settings\/general$/),
    );

    await screen.getByRole('button', { name: 'Actions for “Mobile”' }).click();
    const mobile = screen.getByRole('menu', { name: 'Actions for “Mobile”' });
    await expect.element(mobile).toBeVisible();
    expect(
      mobile
        .getByRole('menuitem')
        .elements()
        .map((item) => item.textContent.trim()),
    ).toEqual(['Open project']);
    await mobile.getByRole('menuitem', { name: 'Open project' }).click();
    expect(onnavigate).toHaveBeenLastCalledWith(expect.stringMatching(/\/projects\/MOB$/));
  });

  it('PRJ-061: archiving from the row menu explains the effects first', async () => {
    let archived = false;
    const calls = server(ADMIN, {
      'GET /projects': projectsRoute((section) => {
        if (section === 'active') return archived ? [MOBILE] : [MOBILE, WEB];
        if (section === 'archived')
          return archived ? [{ ...WEB, archived_at: 'x', version: 2 }] : [];
        return [];
      }),
      'POST /projects/1/archive': () => {
        archived = true;
        return { body: { ...WEB, archived_at: '2026-10-22T10:00:00Z', version: 2 } };
      },
    });
    const screen = await listPage();

    await screen.getByRole('button', { name: 'Actions for “Web”' }).click();
    await screen.getByRole('menuitem', { name: 'Archive project' }).click();
    const dialog = page.getByRole('dialog', { name: 'Archive “Web”?' });
    await expect
      .element(dialog.getByText('The project becomes read-only', { exact: false }))
      .toBeVisible();
    await expect.element(dialog.getByText('Nothing is deleted.', { exact: false })).toBeVisible();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Archive project' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(
      calls.some((call) => call.method === 'POST' && call.path === '/projects/1/archive'),
    ).toBe(true);
    await expect
      .element(screen.getByRole('list', { name: 'Projects' }).getByRole('link', { name: 'Web' }))
      .not.toBeInTheDocument();
    // The list catches up with the server.
    await expect
      .element(screen.getByRole('list', { name: 'Projects' }))
      .toHaveAttribute('aria-busy', 'false');
  });

  it('PRJ-064, PRJ-065: deleting from the row menu needs the exact name, then shows the job', async () => {
    let deleted = false;
    const calls = server(ADMIN, {
      'GET /projects': projectsRoute((section) => {
        if (section === 'active') return deleted ? [MOBILE] : [MOBILE, WEB];
        if (section === 'deleting')
          return deleted
            ? [{ ...WEB, deletion: { state: 'queued', error: null }, version: 2 }]
            : [];
        return [];
      }),
      'DELETE /projects/1': () => {
        deleted = true;
        return { status: 202 };
      },
    });
    const screen = await listPage({ deleting: true });

    await screen.getByRole('button', { name: 'Actions for “Web”' }).click();
    await screen.getByRole('menuitem', { name: 'Delete project' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete “Web”?' });
    await expect.element(dialog.getByText('This cannot be undone', { exact: false })).toBeVisible();
    await expect
      .element(dialog.getByText('Deletion runs in the background', { exact: false }))
      .toBeVisible();
    await dialog.getByLabelText('Type the project name to confirm').fill('web');
    await dialog.getByRole('button', { name: 'Delete project' }).click();
    await expect.element(dialog.getByText('The name does not match.')).toBeVisible();
    expect(calls.some((call) => call.method === 'DELETE')).toBe(false);
    await expectAccessible(dialog.element());

    await dialog.getByLabelText('Type the project name to confirm').fill('Web');
    await dialog.getByRole('button', { name: 'Delete project' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    const beingDeleted = screen.getByRole('list', { name: 'Projects being deleted' });
    await expect.element(beingDeleted.getByText('Web', { exact: true })).toBeVisible();
    await expect.element(beingDeleted.getByText('Waiting to start')).toBeVisible();
    expect(rowNames(screen, 'Projects')).toEqual(['Mobile']);
    await expect
      .element(screen.getByRole('list', { name: 'Projects' }))
      .toHaveAttribute('aria-busy', 'false');
  });

  it('PRJ-013, PRJ-063: the archived section loads when opened and restores a project', async () => {
    let restored = false;
    const calls = server(LEAD, {
      'GET /projects': projectsRoute((section) => {
        if (section === 'active') return restored ? [MOBILE, WEB] : [MOBILE];
        if (section === 'archived')
          return restored ? [OLD] : [OLD, { ...WEB, archived_at: '2026-10-21T09:00:00Z' }];
        return [];
      }),
      'POST /projects/1/restore': () => {
        restored = true;
        return { body: { ...WEB, version: 3 } };
      },
    });
    const screen = await listPage();
    const before = calls.length;

    const toggle = screen.getByRole('button', { name: 'Archived' });
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByRole('list', { name: 'Archived projects' }).query()).toBeNull();
    await toggle.click();
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'true');
    const archived = screen.getByRole('list', { name: 'Archived projects' });
    await expect.element(archived.getByRole('link', { name: 'Legacy' })).toBeVisible();
    expect(calls.slice(before).some((call) => call.query.get('section') === 'archived')).toBe(true);
    // No star and no menu for archived projects; restore only where the person may.
    expect(archived.getByRole('button', { name: /favorites/ }).query()).toBeNull();
    expect(archived.getByRole('button', { name: 'Restore “Legacy”' }).query()).toBeNull();
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await archived.getByRole('button', { name: 'Restore “Web”' }).click();
    await expect.element(archived.getByRole('link', { name: 'Web' })).not.toBeInTheDocument();
    await vi.waitFor(() => {
      expect(rowNames(screen, 'Projects')).toEqual(['Mobile', 'Web']);
    });
  });

  it('PRJ-065: projects being deleted show the job’s state; a failed one can be deleted again', async () => {
    let restarted = false;
    const queued = project(5, 'Queued', 'QUE', { deletion: { state: 'queued', error: null } });
    const running = project(6, 'Running', 'RUN', { deletion: { state: 'running', error: null } });
    const failed = project(7, 'Broken', 'BRK', {
      deletion: { state: 'failed', error: 'storage is not reachable' },
    });
    const calls = server(ADMIN, {
      'GET /projects': projectsRoute((section) => {
        if (section === 'active') return [WEB];
        if (section === 'deleting')
          return [
            queued,
            running,
            restarted
              ? { ...failed, deletion: { state: 'running', error: null }, version: 2 }
              : failed,
          ];
        return [];
      }),
      'DELETE /projects/7': () => {
        restarted = true;
        return { status: 202 };
      },
    });
    const screen = await listPage({ deleting: true });

    await expect
      .element(screen.getByRole('heading', { level: 2, name: 'Being deleted' }))
      .toBeVisible();
    const list = screen.getByRole('list', { name: 'Projects being deleted' });
    await expect.element(list.getByText('Waiting to start')).toBeVisible();
    await expect.element(list.getByText('Deleting…')).toBeVisible();
    await expect.element(list.getByText('Deletion failed')).toBeVisible();
    await expect.element(list.getByText('storage is not reachable')).toBeVisible();
    // They cannot be opened any more.
    expect(list.getByRole('link').query()).toBeNull();
    expect(list.getByRole('button').elements()).toHaveLength(1);
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await list.getByRole('button', { name: 'Delete again: Broken' }).click();
    await expect
      .element(list.getByRole('button', { name: 'Delete again: Broken' }))
      .not.toBeInTheDocument();
    expect(calls.some((call) => call.method === 'DELETE' && call.path === '/projects/7')).toBe(
      true,
    );
    await expect.element(list.getByText('storage is not reachable')).not.toBeInTheDocument();
  });

  it('PRJ-010: searches by name or key a moment after typing, and says when nothing matches', async () => {
    const calls = server(ADMIN, {
      'GET /projects': projectsRoute((section, q) => {
        if (section !== 'active') return [];
        if (!q) return [MOBILE, WEB];
        return [MOBILE, WEB].filter(
          (one) =>
            one.name.toLowerCase().includes(q.toLowerCase()) || one.key.startsWith(q.toUpperCase()),
        );
      }),
    });
    const onsearch = vi.fn();
    const screen = await listPage({ onsearch });

    await screen.getByLabelText('Find projects').fill('mo');
    await vi.waitFor(() => {
      expect(rowNames(screen, 'Projects')).toEqual(['Mobile']);
    });
    expect(onsearch).toHaveBeenLastCalledWith('mo');
    expect(calls.some((call) => call.query.get('q') === 'mo')).toBe(true);

    await screen.getByLabelText('Find projects').fill('zzz-none');
    await expect.element(screen.getByRole('heading', { name: 'No projects found' })).toBeVisible();
    expect(onsearch).toHaveBeenLastCalledWith('zzz-none');
    await expectAccessible(screen.container);
  });

  it('PRJ-010: with no projects yet, the page offers creating the first one', async () => {
    server(ADMIN, {
      'GET /projects': projectsRoute(() => []),
    });
    const screen = await listPage({ query: 'nothing-yet-a' });
    await expect.element(screen.getByRole('heading', { name: 'No projects found' })).toBeVisible();
    await screen.unmount();

    const empty = await listPage();
    await expect
      .element(empty.getByRole('heading', { level: 2, name: 'No projects yet' }))
      .toBeVisible();
    expect(empty.getByRole('link', { name: 'Create project' }).elements()).toHaveLength(2);
    expect(smallTargets(empty.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(empty.container);
  });
});
