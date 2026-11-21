// A project's fields page on a desktop and on a phone (step 1.2; FLD-001, FLD-009, FLD-030,
// FLD-031, FLD-051…FLD-058, FLD-060).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Project } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import ProjectFieldsPage from './ProjectFieldsPage.svelte';
import { attachment, fieldsServer, storedField, storedSet, value, writes } from './test-server.ts';

const ADMIN: AccessMap = {
  global: ['user.directory'],
  projects: [{ project_id: 1, permissions: ['project.full', 'project.update', 'project.basic'] }],
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

/** Priority (shared set), Assignee (user), State, Fix versions and Due date in Alpha. */
function server(extra: { lockPriorities?: boolean } = {}) {
  return fieldsServer({
    issues: { fields: { 4: 3 } },
    fields: [
      storedField(1, 'Priority', 'enum', { setId: 10, aliases: ['prio'] }),
      storedField(2, 'Assignee', 'user', { setId: 11 }),
      storedField(3, 'State', 'state', { setId: 12 }),
      storedField(4, 'Fix versions', 'version', { setId: 13, multi: true }),
      storedField(5, 'Due date', 'date'),
      storedField(6, 'Component', 'owned', { setId: 14 }),
      storedField(7, 'Review state', 'state', { setId: 15 }),
    ],
    sets: [
      storedSet(
        10,
        'Priorities',
        'enum',
        [value(100, 'High', { color: 1 }), value(101, 'Normal', { color: 8 }), value(102, 'Low')],
        { locked: extra.lockPriorities ?? false },
      ),
      storedSet(11, 'Alpha people', 'user', [], { users: [7], groups: [31] }),
      storedSet(12, 'States', 'state', [
        value(120, 'Open'),
        value(121, 'Done', { resolved: true }),
      ]),
      storedSet(13, 'Versions', 'version', [value(130, '1.0'), value(131, '2.0')]),
      storedSet(14, 'Components', 'owned', [value(140, 'Backend')]),
      storedSet(15, 'Review states', 'state', [value(150, 'Waiting')]),
    ],
    projects: {
      1: [
        attachment(1, { setId: 10, defaults: [101], can_be_empty: false }),
        attachment(2, { setId: 11, empty_label: 'Unassigned', private: true }),
        attachment(3, { setId: 12, defaults: [120], can_be_empty: false }),
        attachment(4, { setId: 13 }),
        attachment(5, { can_be_empty: false }),
      ],
      2: [attachment(1, { setId: 10 })],
    },
  });
}

function rowNames(): string[] {
  return page
    .getByRole('list', { name: 'Fields of the project' })
    .getByRole('listitem')
    .elements()
    .map((item) => item.querySelector('button.name')?.textContent.trim() ?? '');
}

/** The settings of a field: a side panel on wide screens, a sheet on phones. */
function panel(name: string) {
  const title = `Settings of “${name}”`;
  return isPhone()
    ? page.getByRole('dialog', { name: title })
    : page.getByRole('complementary', { name: title });
}

async function menuAction(field: string, action: string): Promise<void> {
  await page.getByRole('button', { name: `Actions for “${field}”` }).click();
  await page.getByRole('menuitem', { name: action }).click();
}

afterEach(() => {
  vi.unstubAllGlobals();
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
  store.entities.table('value_set').clear();
  store.entities.table('field').clear();
});

describe('ProjectFieldsPage', () => {
  it('FLD-051, FLD-058, FLD-009, FLD-057, FLD-060: lists fields in order with type, values, defaults, empty handling and privacy; Move up and down change the order', async () => {
    setAccessForTests(ADMIN);
    const { calls } = server();
    const screen = await render(ProjectFieldsPage, { project: project() });

    await expect.element(screen.getByRole('button', { name: 'Priority' })).toBeVisible();
    expect(rowNames()).toEqual(['Priority', 'Assignee', 'State', 'Fix versions', 'Due date']);
    const priority = screen.getByRole('button', { name: 'Priority', exact: true });
    await expect
      .element(priority)
      .toHaveAccessibleDescription(
        'Choice · Values: Priorities (shared with 1 other project) Default: Normal Cannot be empty',
      );
    await expect
      .element(screen.getByRole('button', { name: 'Assignee', exact: true }))
      .toHaveAccessibleDescription(
        'User · Values: Alpha people Default: No default value Empty: “Unassigned”',
      );
    const assigneeRow = screen.getByRole('listitem').filter({ hasText: 'Assignee' });
    await expect.element(assigneeRow.getByText('Private', { exact: true })).toBeVisible();
    await expect
      .element(screen.getByRole('button', { name: 'Fix versions', exact: true }))
      .toHaveAccessibleDescription(/^Version · several values/);
    const dueRow = screen.getByRole('listitem').filter({ hasText: 'Due date' });
    await expect.element(dueRow.getByText('Required', { exact: true })).toBeVisible();
    await expect.element(screen.getByText(/several state fields/)).not.toBeInTheDocument();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await menuAction('Assignee', 'Move up');
    await vi.waitFor(() => {
      expect(rowNames().slice(0, 2)).toEqual(['Assignee', 'Priority']);
    });
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'POST',
      path: '/projects/1/fields/2/move',
      body: { after: null },
    });

    await menuAction('Assignee', 'Move down');
    await vi.waitFor(() => {
      expect(rowNames().slice(0, 2)).toEqual(['Priority', 'Assignee']);
    });
    expect(writes(calls).at(-1)?.body).toEqual({ after: 1 });

    await page.getByRole('button', { name: 'Actions for “Priority”' }).click();
    await expect.element(page.getByRole('menuitem', { name: 'Move up' })).toBeDisabled();
    await userEvent.keyboard('{Escape}');

    if (!isPhone()) {
      // A mouse drags a row by its grip to land after another one.
      const grips = screen.container.querySelectorAll<HTMLElement>('.grip');
      const rows = screen.container.querySelectorAll<HTMLElement>('li.row');
      const dueGrip = grips[4];
      const stateRow = rows[2];
      if (!dueGrip || !stateRow) throw new Error('rows are missing');
      await userEvent.dragAndDrop(dueGrip, stateRow, { targetPosition: { x: 40, y: 4 } });
      await vi.waitFor(() => {
        expect(rowNames()).toEqual(['Priority', 'Assignee', 'Due date', 'State', 'Fix versions']);
      });
      expect(writes(calls).at(-1)).toMatchObject({
        path: '/projects/1/fields/5/move',
        body: { after: 2 },
      });
    } else {
      // Touch screens move rows through the menu only.
      const grip = screen.container.querySelector<HTMLElement>('.grip');
      expect(grip?.checkVisibility()).toBe(false);
    }
  });

  it('FLD-053, FLD-030: adds an existing field, sharing or copying its values, and opens its settings', async () => {
    setAccessForTests(ADMIN);
    const { calls } = server();
    const screen = await render(ProjectFieldsPage, { project: project() });
    await expect.element(screen.getByRole('button', { name: 'Priority' })).toBeVisible();

    await screen.getByRole('button', { name: 'Add field' }).first().click();
    const dialog = page.getByRole('dialog', { name: 'Add field' });
    await expect.element(dialog.getByRole('radio', { name: /^Component/ })).toBeVisible();
    await expect.element(dialog.getByRole('radio', { name: /^Priority/ })).toBeDisabled();
    await expect
      .element(dialog.getByText('In this project', { exact: true }).first())
      .toBeVisible();
    await dialog.getByLabelText('Find fields').fill('comp');
    await expect.element(dialog.getByRole('radio', { name: /^Priority/ })).not.toBeInTheDocument();
    await dialog.getByRole('radio', { name: /^Component/ }).click();
    await expect
      .element(dialog.getByRole('radio', { name: 'Share the field’s values (Components)' }))
      .toBeChecked();
    await dialog.getByRole('radio', { name: 'Copy them for this project' }).click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Add field' }).click();

    await vi.waitFor(() => {
      expect(writes(calls).at(-1)).toMatchObject({
        method: 'POST',
        path: '/projects/1/fields',
        body: { source: 'existing', field_id: 6, set: 'copy' },
      });
    });
    await expect.element(panel('Component')).toBeVisible();
    expect(toasts.items.at(-1)?.message).toBe('Field added to the project');
    await expect
      .element(panel('Component').getByText(/Value set: Components \(copy\)/))
      .toBeVisible();
    expect(rowNames().at(-1)).toBe('Component');
  });

  it('FLD-052, FLD-001: creates a new field for the project; only some types hold several values', async () => {
    setAccessForTests(ADMIN);
    const { calls } = server();
    const screen = await render(ProjectFieldsPage, { project: project() });
    await expect.element(screen.getByRole('button', { name: 'Priority' })).toBeVisible();

    await screen.getByRole('button', { name: 'Add field' }).first().click();
    const dialog = page.getByRole('dialog', { name: 'Add field' });
    await dialog.getByRole('radio', { name: 'A new field' }).click();
    await dialog.getByRole('button', { name: 'Create field' }).click();
    await expect.element(dialog.getByText('Fill in this field.')).toBeVisible();

    await dialog.getByLabelText(/^Name/).fill('Priority');
    await dialog.getByLabelText('Type').selectOptions('state');
    await expect
      .element(dialog.getByRole('checkbox', { name: 'Several values' }))
      .not.toBeInTheDocument();
    await dialog.getByLabelText('Type').selectOptions('enum');
    await dialog.getByRole('button', { name: 'Create field' }).click();
    await expect
      .element(dialog.getByText('A field of this type with this name exists already.'))
      .toBeVisible();

    await dialog.getByLabelText(/^Name/).fill('Release');
    await dialog.getByLabelText('Type').selectOptions('version');
    await dialog.getByRole('checkbox', { name: 'Several values' }).click();
    await dialog.getByLabelText('Aliases').fill('ships in, ships in, release');
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Create field' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({
        source: 'new',
        name: 'Release',
        kind: 'version',
        multi: true,
        aliases: ['ships in', 'release'],
      });
    });
    await expect.element(panel('Release')).toBeVisible();
    expect(toasts.items.at(-1)?.message).toBe('Field created and added to the project');
  });

  it('FLD-055, FLD-056, FLD-057, FLD-060: changes empty handling, the default value and privacy in a panel or a phone sheet', async () => {
    setAccessForTests(ADMIN);
    const { calls } = server();
    const screen = await render(ProjectFieldsPage, { project: project() });
    await screen.getByRole('button', { name: 'Priority', exact: true }).click();

    const settings = panel('Priority');
    await expect.element(settings).toBeVisible();
    if (isPhone()) {
      await expect
        .element(page.getByRole('complementary', { name: 'Settings of “Priority”' }))
        .not.toBeInTheDocument();
    }
    await expect.element(settings.getByText('Aliases: prio')).toBeVisible();
    const canBeEmpty = settings.getByRole('checkbox', { name: 'Can be empty' });
    await expect.element(canBeEmpty).not.toBeChecked();
    await expect.element(settings.getByLabelText('Empty value label')).toBeDisabled();
    await expect.element(settings.getByRole('radio', { name: 'Normal' })).toBeChecked();
    await expect.element(settings.getByRole('button', { name: 'Save changes' })).toBeDisabled();

    // No default and cannot be empty: required (FLD-057).
    await settings.getByRole('radio', { name: 'No default value' }).click();
    await expect.element(settings.getByText(/^Required: it cannot be empty/)).toBeVisible();
    await settings.getByRole('radio', { name: 'High' }).click();
    await expect
      .element(settings.getByText(/^Required: it cannot be empty/))
      .not.toBeInTheDocument();

    await canBeEmpty.click();
    await settings.getByLabelText('Empty value label').fill('Not set');
    const hidden = settings.getByRole('checkbox', { name: 'Private field' });
    await expect
      .element(hidden)
      .toHaveAccessibleDescription(
        'Only people whose roles include “Read private fields” see it, and changing it needs “Edit private fields”.',
      );
    await hidden.click();
    await expectAccessible(settings.element());
    await settings.getByRole('button', { name: 'Save changes' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)).toMatchObject({
        method: 'PATCH',
        path: '/projects/1/fields/1',
        body: {
          can_be_empty: true,
          empty_label: 'Not set',
          private: true,
          default_values: [100],
          version: 1,
        },
      });
    });
    expect(toasts.items.at(-1)?.message).toBe('Settings saved');
    if (isPhone()) await page.getByRole('button', { name: 'Close' }).click();
    else await page.getByRole('button', { name: 'Close settings' }).click();
    await expect
      .element(screen.getByRole('button', { name: 'Priority', exact: true }))
      .toHaveAccessibleDescription(/Default: High Empty: “Not set”$/);
    await expect
      .element(screen.getByRole('listitem').filter({ hasText: 'Priority' }).getByText('Private'))
      .toBeVisible();
  });

  it('FLD-055, FLD-010: a user field takes default people; the server refuses someone its set does not offer', async () => {
    setAccessForTests(ADMIN);
    const { calls } = server();
    const screen = await render(ProjectFieldsPage, { project: project() });
    await screen.getByRole('button', { name: 'Assignee', exact: true }).click();
    const settings = panel('Assignee');
    await expect.element(settings.getByText('Nobody by default.')).toBeVisible();

    await settings.getByRole('button', { name: 'Choose a person' }).click();
    const picker = page.getByRole('dialog', { name: 'Choose a person' });
    await picker.getByRole('radio', { name: /Bob Stone/ }).click();
    await picker.getByRole('button', { name: 'Choose' }).click();
    await expect.element(settings.getByText('Bob Stone')).toBeVisible();
    await settings.getByRole('button', { name: 'Save changes' }).click();
    await expect
      .element(settings.getByText(/A default value must be one the field offers/))
      .toBeVisible();

    await settings.getByRole('button', { name: 'Remove Bob Stone from the default' }).click();
    await settings.getByRole('button', { name: 'Choose a person' }).click();
    await picker.getByRole('radio', { name: /Ann Lee/ }).click();
    await picker.getByRole('button', { name: 'Choose' }).click();
    await settings.getByRole('button', { name: 'Save changes' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({ default_values: [7], version: 1 });
    });
    // The set's people and groups are listed, a team by its project's name (FLD-040).
    await expect.element(settings.getByText('Alpha team')).toBeVisible();
  });

  it('FLD-054: removes a field from the project after a confirmation that counts the issues losing values', async () => {
    setAccessForTests(ADMIN);
    const { calls } = server();
    const screen = await render(ProjectFieldsPage, { project: project() });
    await expect.element(screen.getByRole('button', { name: 'Priority' })).toBeVisible();

    await menuAction('Fix versions', 'Remove from project');
    const dialog = page.getByRole('dialog', { name: 'Remove “Fix versions” from the project?' });
    await expect
      .element(
        dialog.getByText(
          'Its values are erased in 3 issues of the project. Adding the field again does not bring them back.',
        ),
      )
      .toBeVisible();
    expect(calls.some((call) => call.path === '/projects/1/fields/4/detach-impact')).toBe(true);
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Remove field' }).click();
    await vi.waitFor(() => {
      expect(rowNames()).not.toContain('Fix versions');
    });
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'DELETE',
      path: '/projects/1/fields/4',
    });
    expect(toasts.items.at(-1)?.message).toBe('Field removed from the project');
  });

  it('FLD-054: a field no issue fills says nothing is lost; Russian texts', async () => {
    setAccessForTests(ADMIN);
    setLocale('ru');
    server();
    const screen = await render(ProjectFieldsPage, { project: project() });
    await expect.element(screen.getByRole('button', { name: 'Due date' })).toBeVisible();
    await page.getByRole('button', { name: 'Действия с полем «Due date»' }).click();
    await page.getByRole('menuitem', { name: 'Убрать из проекта' }).click();
    const dialog = page.getByRole('dialog', { name: 'Убрать поле «Due date» из проекта?' });
    await expect
      .element(
        dialog.getByText(
          'Ни в одной задаче проекта оно не заполнено, так что ничего не потеряется.',
        ),
      )
      .toBeVisible();
    await expect.element(dialog.getByRole('button', { name: 'Убрать поле' })).toBeEnabled();
    if (isPhone()) expect(await smallTargets(dialog.element())).toEqual([]);
  });

  it('FLD-009: warns when a project has several state fields', async () => {
    setAccessForTests(ADMIN);
    const state = fieldsServer({
      fields: [storedField(3, 'State', 'state'), storedField(7, 'Review state', 'state')],
      projects: { 1: [attachment(3), attachment(7)] },
    });
    expect(state.fields.size).toBe(2);
    const screen = await render(ProjectFieldsPage, { project: project() });
    await expect.element(screen.getByText(/This project has several state fields/)).toBeVisible();
  });

  it('FLD-051: an archived project shows its fields read-only', async () => {
    setAccessForTests(ADMIN);
    const { calls } = server();
    const screen = await render(ProjectFieldsPage, {
      project: project({ archived_at: '2026-10-20T10:00:00Z' }),
    });
    await expect.element(screen.getByRole('button', { name: 'Priority' })).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Add field' })).not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'Actions for “Priority”' }))
      .not.toBeInTheDocument();
    expect(screen.container.querySelector('.grip')).toBeNull();

    await screen.getByRole('button', { name: 'Priority', exact: true }).click();
    const settings = panel('Priority');
    await expect.element(settings.getByRole('checkbox', { name: 'Can be empty' })).toBeDisabled();
    await expect.element(settings.getByRole('radio', { name: 'High' })).toBeDisabled();
    await expect
      .element(settings.getByRole('button', { name: 'Save changes' }))
      .not.toBeInTheDocument();
    await expect
      .element(settings.getByRole('list', { name: 'Values of “Priorities”' }).getByText('High'))
      .toBeVisible();
    await expect
      .element(settings.getByRole('button', { name: 'Add value' }))
      .not.toBeInTheDocument();
    await expect
      .element(settings.getByRole('button', { name: 'Remove from project' }))
      .not.toBeInTheDocument();
    await expectAccessible(settings.element());
    expect(writes(calls)).toEqual([]);
  });

  it('FLD-031, FLD-030: a shared set the person may not change explains why and offers a copy for the project', async () => {
    setAccessForTests(ADMIN);
    const { calls } = server({ lockPriorities: true });
    const screen = await render(ProjectFieldsPage, { project: project() });
    await screen.getByRole('button', { name: 'Priority', exact: true }).click();
    const settings = panel('Priority');
    await expect
      .element(settings.getByText(/projects you do not configure use them too/))
      .toBeVisible();
    await expect.element(settings.getByText('Used in: Alpha, Beta.')).toBeVisible();
    await expect
      .element(settings.getByRole('button', { name: 'Add value' }))
      .not.toBeInTheDocument();

    await settings.getByRole('button', { name: 'Make a copy for this project' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)).toMatchObject({
        method: 'POST',
        path: '/projects/1/fields/1/fork-set',
      });
    });
    await expect.element(settings.getByText(/Value set: Priorities \(Alpha\)/)).toBeVisible();
    await expect.element(settings.getByRole('button', { name: 'Add value' })).toBeVisible();
    expect(toasts.items.at(-1)?.message).toBe('The project now has its own copy of the values');
  });

  it('FLD-031: changing a shared set warns that every project using it changes', async () => {
    setAccessForTests(ADMIN);
    server();
    const screen = await render(ProjectFieldsPage, { project: project() });
    await screen.getByRole('button', { name: 'Priority', exact: true }).click();
    const settings = panel('Priority');
    await expect
      .element(
        settings
          .getByText('Changes to these values apply to all 2 projects using the set.')
          .first(),
      )
      .toBeVisible();
    await expect
      .element(settings.getByRole('button', { name: 'Make a copy for this project' }))
      .toBeVisible();
  });
});
