// The rules tab of a project's settings (AUT-023): the registry with toggles, what a rule lacks,
// the conflict path and the missing right, on a desktop and on a phone.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Project, RulesView } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi, failure, type FakeRequest } from '../../../../test/fake-api.ts';
import RulesTab from './RulesTab.svelte';

const ADMIN: AccessMap = {
  global: ['user.directory'],
  projects: [{ project_id: 1, permissions: ['project.full', 'project.update', 'project.basic'] }],
  capabilities: [],
};

/** Sees the project, cannot configure it. */
const READER: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic'] }],
  capabilities: [],
};

function project(extra: Partial<Project> = {}): Project {
  return {
    id: 1,
    key: 'ALPHA',
    name: 'Alpha',
    description: '',
    description_html: '',
    archived_at: null,
    created_at: '2026-10-01T09:00:00Z',
    default_audience: null,
    deletion: null,
    favorite: false,
    former_keys: [],
    kind: 'standard',
    logo_color: 1,
    logo_url: null,
    next_number: 1,
    owner: null,
    team: null,
    team_size: 1,
    template: 'blank',
    version: 1,
    ...extra,
  };
}

/** Three rules: one running, one off, one the project cannot run yet. */
function rules(): RulesView {
  return {
    version: 4,
    items: [
      {
        rule: 'parent_follows_subtasks',
        name: 'Parent follows subtasks',
        enabled: true,
        needs_setup: [],
      },
      {
        rule: 'component_owner',
        name: 'Component owner',
        enabled: false,
        needs_setup: [
          { code: 'field', params: { kind: 'enum', name: 'Components' } },
          { code: 'value', params: { field: 'State' } },
        ],
      },
      {
        rule: 'plus_one_vote',
        name: '+1 counts as a vote',
        enabled: true,
        needs_setup: [{ code: 'value', params: { field: 'State', value: 'Duplicate' } }],
      },
    ],
  };
}

/** The registry as the server answers it after the first rule is turned off. */
function toggled(): RulesView {
  const answer = rules();
  const items = answer.items.map((row) =>
    row.rule === 'parent_follows_subtasks' ? { ...row, enabled: false } : row,
  );
  return { ...answer, items };
}

function writes(calls: FakeRequest[]): FakeRequest[] {
  return calls.filter((call) => call.method !== 'GET');
}

afterEach(() => {
  vi.unstubAllGlobals();
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
  store.entities.table('issue').clear();
});

describe('AUT-023: правила — вкладка', () => {
  it('lists the registry with switches; a switch PATCHes the rule with the version', async () => {
    setAccessForTests(ADMIN);
    const answer = toggled();
    const calls = fakeApi({
      'GET /projects/1/rules': { body: rules() },
      'PATCH /projects/1/rules': { body: answer },
    });
    const screen = await render(RulesTab, { project: project() });

    const first = page.getByRole('switch', { name: 'Parent follows subtasks' });
    await expect.element(first).toHaveAttribute('aria-checked', 'true');
    await expect
      .element(page.getByRole('switch', { name: 'Component owner' }))
      .toHaveAttribute('aria-checked', 'false');

    await first.click();
    await expect.element(first).toHaveAttribute('aria-checked', 'false');
    expect(writes(calls)).toEqual([
      {
        method: 'PATCH',
        path: '/projects/1/rules',
        query: new URLSearchParams(),
        body: { rule: 'parent_follows_subtasks', enabled: false, version: 4 },
      },
    ]);
    // The answer's version replaces the tab's.
    const second = page.getByRole('switch', { name: 'Parent follows subtasks' });
    await second.click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({
        rule: 'parent_follows_subtasks',
        enabled: true,
        version: answer.version,
      });
    });

    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('a conflict reloads the tab, puts the switch back and asks to repeat', async () => {
    setAccessForTests(ADMIN);
    let reloads = 0;
    const calls = fakeApi({
      'GET /projects/1/rules': () => {
        reloads += 1;
        return { body: rules() };
      },
      'PATCH /projects/1/rules': failure(409, 'conflict'),
    });
    await render(RulesTab, { project: project() });

    await page.getByRole('switch', { name: 'Parent follows subtasks' }).click();
    await vi.waitFor(() => {
      expect(reloads).toBe(2);
    });
    await expect
      .element(page.getByRole('switch', { name: 'Parent follows subtasks' }))
      .toHaveAttribute('aria-checked', 'true');
    expect(toasts.items.map((toast) => toast.message)).toEqual(['The settings changed, try again']);
    expect(writes(calls).length).toBe(1);
  });

  it('what a rule lacks shows as a gray note with a link to the fields tab', async () => {
    setAccessForTests(ADMIN);
    fakeApi({ 'GET /projects/1/rules': { body: rules() } });
    const screen = await render(RulesTab, { project: project() });

    const ownerRow = page.getByRole('listitem').filter({ hasText: 'Component owner' });
    await expect
      .element(ownerRow.getByText(/Needs setup:/))
      .toHaveTextContent(
        'Needs setup: missing the field “Components”; the set of “State” lacks values. Open “Fields”',
      );
    const link = ownerRow.getByRole('link', { name: 'Open “Fields”' });
    await expect.element(link).toHaveAttribute('href', '/projects/ALPHA/settings/fields');
    await expect.element(screen.getByText(/lacks the value “Duplicate”/)).toBeVisible();
    await expectAccessible(screen.container);
  });

  it('AUT-085: the scrum rule says what the project still lacks — fields and values', async () => {
    setAccessForTests(ADMIN);
    fakeApi({
      'GET /projects/1/rules': {
        body: {
          version: 4,
          items: [
            {
              rule: 'scrum',
              name: 'Scrum',
              enabled: true,
              needs_setup: [
                { code: 'field', params: { kind: 'float', name: 'Story points' } },
                { code: 'value', params: { field: 'Type', value: 'Epic' } },
              ],
            },
          ],
        },
      },
    });
    const screen = await render(RulesTab, { project: project() });

    const row = page.getByRole('listitem').filter({ hasText: 'Scrum' });
    await expect
      .element(row.getByText(/Needs setup:/))
      .toHaveTextContent(
        'Needs setup: missing the field “Story points”; the set of “Type” lacks the value “Epic”. Open “Fields”',
      );
    await expect
      .element(row.getByRole('link', { name: 'Open “Fields”' }))
      .toHaveAttribute('href', '/projects/ALPHA/settings/fields');
    await expectAccessible(screen.container);
  });

  it('without the settings right every switch is disabled and says so', async () => {
    setAccessForTests(READER);
    fakeApi({ 'GET /projects/1/rules': { body: rules() } });
    const screen = await render(RulesTab, { project: project() });

    for (const name of ['Parent follows subtasks', 'Component owner', '+1 counts as a vote']) {
      const control = page.getByRole('switch', { name });
      await expect.element(control).toBeDisabled();
      await expect
        .element(control)
        .toHaveAttribute('title', 'You cannot change this project’s settings');
    }
    await expectAccessible(screen.container);
  });
});
