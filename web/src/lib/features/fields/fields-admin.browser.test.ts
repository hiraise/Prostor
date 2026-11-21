// The fields of the instance on a desktop and on a phone (step 1.2; FLD-001…FLD-007, FLD-011,
// FLD-015…FLD-020, FLD-022, FLD-023, FLD-025, FLD-027, FLD-070).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import FieldPage from './FieldPage.svelte';
import { ensureField, ensureFields } from './fields.ts';
import FieldsPage from './FieldsPage.svelte';
import {
  attachment,
  fieldsServer,
  storedField,
  storedSet,
  value,
  writes,
  type BoardUse,
} from './test-server.ts';

const SYSTEM: AccessMap = {
  global: ['system.read', 'system.write', 'user.directory'],
  projects: [],
  capabilities: [],
};

const PROJECT_ADMIN: AccessMap = {
  global: ['user.directory'],
  projects: [{ project_id: 1, permissions: ['project.update', 'project.full'] }],
  capabilities: [],
};

function server(boards: Record<number, { items: BoardUse[]; others?: number }> = {}) {
  return fieldsServer({
    boards,
    issues: { fields: { 1: 120 } },
    fields: [
      storedField(1, 'Priority', 'enum', {
        setId: 10,
        builtin: 'priority',
        auto_attach: true,
        aliases: ['prio'],
        defaults: [101],
      }),
      storedField(2, 'Assignee', 'user', { setId: 11, builtin: 'assignee', auto_attach: true }),
      storedField(3, 'Due date', 'date', { builtin: 'due_date' }),
      storedField(4, 'Reviewers', 'group', { multi: true, private: true }),
      storedField(5, 'Severity', 'enum', { setId: 12 }),
    ],
    sets: [
      storedSet(10, 'Priorities', 'enum', [value(100, 'High'), value(101, 'Normal')]),
      storedSet(11, 'Assignees', 'user', [], { users: [7] }),
      storedSet(12, 'Severities', 'enum', [value(120, 'Major'), value(121, 'Minor')]),
    ],
    projects: {
      1: [attachment(1, { setId: 10 }), attachment(2, { setId: 11 })],
      2: [attachment(1, { setId: 10 })],
      9: [attachment(5, { setId: 12 })],
    },
  });
}

function fieldNames(): string[] {
  return page
    .getByRole('list', { name: 'Fields' })
    .getByRole('listitem')
    .elements()
    .map((item) => item.querySelector('a.name')?.textContent.trim() ?? '');
}

async function fieldsList() {
  const list = await ensureFields({ q: '', unused: false });
  await list.refresh();
  return list;
}

afterEach(() => {
  vi.unstubAllGlobals();
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
  store.entities.table('field').clear();
  store.entities.table('value_set').clear();
});

describe('FieldsPage', () => {
  it('FLD-025, FLD-070, FLD-020, FLD-022: lists fields in order with type, values, aliases, badges and use; filters; changes the order', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = server();
    const onfilter = vi.fn();
    const screen = await render(FieldsPage, {
      fields: await fieldsList(),
      oncreated: vi.fn(),
      onfilter,
    });

    expect(fieldNames()).toEqual(['Priority', 'Assignee', 'Due date', 'Reviewers', 'Severity']);
    const priority = screen.getByRole('link', { name: 'Priority' });
    await expect
      .element(priority)
      .toHaveAccessibleDescription('Choice · Values: Priorities Aliases: prio Used in 2 projects');
    await expect.element(priority).toHaveAttribute('href', '/admin/fields/1');
    const priorityRow = screen.getByRole('listitem').filter({ hasText: 'Priority' });
    await expect.element(priorityRow.getByText('Built-in')).toBeVisible();
    await expect.element(priorityRow.getByText('Added to new projects')).toBeVisible();
    await expect
      .element(screen.getByRole('link', { name: 'Reviewers' }))
      .toHaveAccessibleDescription('Group · several values Not used in any project');
    await expect
      .element(
        screen
          .getByRole('listitem')
          .filter({ hasText: 'Reviewers' })
          .getByText('Private by default'),
      )
      .toBeVisible();
    await expect
      .element(screen.getByRole('link', { name: 'Severity' }))
      .toHaveAccessibleDescription(/Used in 1 project$/);
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await screen.getByRole('button', { name: 'Actions for “Severity”' }).click();
    await page.getByRole('menuitem', { name: 'Move up' }).click();
    await vi.waitFor(() => {
      expect(fieldNames()).toEqual(['Priority', 'Assignee', 'Due date', 'Severity', 'Reviewers']);
    });
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'POST',
      path: '/fields/5/move',
      body: { after: 3 },
    });

    if (!isPhone()) {
      const grips = screen.container.querySelectorAll<HTMLElement>('.grip');
      const rows = screen.container.querySelectorAll<HTMLElement>('li.row');
      const reviewersGrip = grips[4];
      const firstRow = rows[0];
      if (!reviewersGrip || !firstRow) throw new Error('rows are missing');
      await userEvent.dragAndDrop(reviewersGrip, firstRow, { targetPosition: { x: 40, y: 3 } });
      await vi.waitFor(() => {
        expect(fieldNames()[0]).toBe('Reviewers');
      });
      expect(writes(calls).at(-1)).toMatchObject({ path: '/fields/4/move', body: { after: null } });
    }

    await screen.getByRole('checkbox', { name: 'Show only unused' }).click();
    await vi.waitFor(() => {
      expect([...fieldNames()].sort()).toEqual(['Due date', 'Reviewers']);
    });
    expect(onfilter).toHaveBeenLastCalledWith({ q: '', unused: true });
    await expect
      .element(screen.getByText('Clear the search and filters to change the order.'))
      .toBeVisible();
    await expect
      .element(screen.getByRole('button', { name: 'Actions for “Reviewers”' }))
      .not.toBeInTheDocument();

    await screen.getByRole('checkbox', { name: 'Show only unused' }).click();
    await screen.getByLabelText('Find fields').fill('prio');
    await vi.waitFor(() => {
      expect(fieldNames()).toEqual(['Priority']);
    });
    await screen.getByLabelText('Find fields').fill('nothing like it');
    await expect.element(screen.getByText('No fields found')).toBeVisible();
  });

  it('FLD-001, FLD-002, FLD-003, FLD-004, FLD-005, FLD-006, FLD-007: offers every type; several values only for types that allow them', async () => {
    setAccessForTests(SYSTEM);
    server();
    const screen = await render(FieldsPage, { fields: await fieldsList(), oncreated: vi.fn() });
    await screen.getByRole('button', { name: 'Create field' }).click();
    const dialog = page.getByRole('dialog', { name: 'New field' });
    const type = dialog.getByLabelText('Type');
    const options = [...(type.element() as HTMLSelectElement).options].map((item) => item.text);
    expect(options).toEqual([
      'Text line',
      'Formatted text',
      'Whole number',
      'Decimal number',
      'Date',
      'Date and time',
      'Duration',
      'Choice',
      'State',
      'User',
      'Group',
      'Version',
      'Build',
      'Value with owner',
    ]);
    for (const kind of [
      'string',
      'text',
      'integer',
      'float',
      'date',
      'date_time',
      'duration',
      'state',
    ]) {
      await type.selectOptions(kind);
      await expect
        .element(dialog.getByRole('checkbox', { name: 'Several values' }))
        .not.toBeInTheDocument();
    }
    for (const kind of ['enum', 'user', 'group', 'version', 'build', 'owned']) {
      await type.selectOptions(kind);
      await expect.element(dialog.getByRole('checkbox', { name: 'Several values' })).toBeVisible();
    }
    await type.selectOptions('duration');
    await expect.element(dialog.getByRole('radio', { name: 'A new set' })).not.toBeInTheDocument();
  });

  it('FLD-015, FLD-016, FLD-019, FLD-020: creates a field with an existing set and default values', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = server();
    const oncreated = vi.fn();
    const screen = await render(FieldsPage, { fields: await fieldsList(), oncreated });
    await screen.getByRole('button', { name: 'Create field' }).click();
    const dialog = page.getByRole('dialog', { name: 'New field' });

    await dialog.getByLabelText(/^Name\s*\*?$/).fill('Impact: high');
    await dialog.getByRole('button', { name: 'Create field' }).click();
    await expect.element(dialog.getByText('A name cannot contain <, >, / or :.')).toBeVisible();

    await dialog.getByLabelText(/^Name\s*\*?$/).fill('Impact');
    await dialog.getByLabelText('Aliases').fill('effect');
    await dialog.getByRole('radio', { name: 'An existing set' }).click();
    await dialog.getByRole('button', { name: 'Create field' }).click();
    await expect
      .element(dialog.getByLabelText(/^Set\s*\*?$/))
      .toHaveAccessibleDescription('Choose a set');
    await dialog.getByLabelText(/^Set\s*\*?$/).selectOptions('12');
    await dialog.getByRole('radio', { name: 'Minor' }).click();
    await dialog.getByRole('checkbox', { name: 'Can be empty' }).click();
    await expect.element(dialog.getByLabelText('Empty value label')).toBeDisabled();
    await dialog.getByRole('checkbox', { name: 'Private by default' }).click();
    await dialog.getByRole('checkbox', { name: 'Add to new projects' }).click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Create field' }).click();
    await vi.waitFor(() => {
      expect(oncreated).toHaveBeenCalledOnce();
    });
    expect(writes(calls).at(-1)?.body).toEqual({
      name: 'Impact',
      kind: 'enum',
      multi: false,
      aliases: ['effect'],
      can_be_empty: false,
      empty_label: '',
      private: true,
      auto_attach: true,
      set: { source: 'existing', set_id: 12 },
      default_values: [121],
    });
    expect(toasts.items.at(-1)?.message).toBe('Field created');
  });

  it('FLD-015: a new set takes the name typed for it, and offers no default values', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = server();
    const screen = await render(FieldsPage, { fields: await fieldsList(), oncreated: vi.fn() });
    await screen.getByRole('button', { name: 'Create field' }).click();
    const dialog = page.getByRole('dialog', { name: 'New field' });
    await dialog.getByLabelText(/^Name\s*\*?$/).fill('Release');
    await dialog.getByLabelText('Type').selectOptions('version');
    await dialog.getByLabelText('Name of the set').fill('Releases');
    await expect
      .element(dialog.getByText('Default value', { exact: true }))
      .not.toBeInTheDocument();
    await dialog.getByRole('button', { name: 'Create field' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toMatchObject({
        kind: 'version',
        set: { source: 'new', name: 'Releases' },
      });
    });
    expect(writes(calls).at(-1)?.body).not.toHaveProperty('default_values');
  });

  it('FLD-027: people who configure projects see the fields without instance-level actions', async () => {
    setAccessForTests(PROJECT_ADMIN);
    server();
    const screen = await render(FieldsPage, { fields: await fieldsList(), oncreated: vi.fn() });
    await expect.element(screen.getByRole('link', { name: 'Priority' })).toBeVisible();
    await expect
      .element(screen.getByRole('button', { name: 'Create field' }))
      .not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'Actions for “Priority”' }))
      .not.toBeInTheDocument();
    expect(screen.container.querySelector('.grip')).toBeNull();
  });

  it('FLD-025: people without access are told whom to ask', async () => {
    setAccessForTests({ global: ['user.directory'], projects: [], capabilities: [] });
    const screen = await render(FieldsPage, { fields: null, oncreated: vi.fn() });
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Only administrators can open this');
  });
});

describe('FieldPage', () => {
  it('FLD-017, FLD-018: changes the name and aliases shared by every project', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = server();
    const screen = await render(FieldPage, { field: await ensureField(5), ondeleted: vi.fn() });
    await expect.element(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Severity');
    await expect
      .element(
        screen.getByText(
          'Shared by every project using the field: a change applies to all of them.',
        ),
      )
      .toBeVisible();
    await screen.getByLabelText(/^Name\s*\*?$/).fill('Impact');
    await screen.getByLabelText('Aliases').fill('effect, sev');
    await screen.getByRole('button', { name: 'Save changes' }).first().click();
    await expect.element(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Impact');
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'PATCH',
      path: '/fields/5',
      body: { name: 'Impact', aliases: ['effect', 'sev'], version: 1 },
    });
    expect(toasts.items.at(-1)?.message).toBe('Field saved');
    await expect
      .element(screen.getByRole('link', { name: 'Open the set “Severities”' }))
      .toHaveAttribute('href', '/admin/value-sets/12');
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('FLD-019, FLD-020: system administrators change what new projects start with', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = server();
    const screen = await render(FieldPage, { field: await ensureField(1), ondeleted: vi.fn() });
    await expect
      .element(screen.getByText(/Projects that use the field already keep their own/))
      .toBeVisible();
    const auto = screen.getByRole('checkbox', { name: 'Add to new projects' });
    await expect.element(auto).toBeChecked();
    await expect.element(screen.getByRole('radio', { name: 'Normal' })).toBeChecked();
    await auto.click();
    await screen.getByRole('radio', { name: 'High' }).click();
    await screen.getByLabelText('Empty value label').fill('No priority');
    await screen.getByRole('button', { name: 'Save changes' }).nth(1).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({
        auto_attach: false,
        empty_label: 'No priority',
        default_values: [100],
        version: 1,
      });
    });
    await expect.element(screen.getByText('Added to new projects')).not.toBeInTheDocument();
  });

  it('FLD-027, FLD-018: a project administrator cannot rename a field new projects get, nor change its starting settings', async () => {
    setAccessForTests(PROJECT_ADMIN);
    server();
    const screen = await render(FieldPage, { field: await ensureField(1), ondeleted: vi.fn() });
    await expect
      .element(screen.getByText(/Only a system administrator can rename a field/))
      .toBeVisible();
    await expect.element(screen.getByLabelText(/^Name\s*\*?$/)).toBeDisabled();
    await expect
      .element(screen.getByRole('checkbox', { name: 'Add to new projects' }))
      .toBeDisabled();
    await expect
      .element(screen.getByRole('button', { name: 'Save changes' }))
      .not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'Delete field' }))
      .not.toBeInTheDocument();
  });

  it('FLD-025: lists the projects using a field, and those the viewer cannot see as a number', async () => {
    setAccessForTests(SYSTEM);
    server();
    const screen = await render(FieldPage, { field: await ensureField(5), ondeleted: vi.fn() });
    await expect.element(screen.getByText('Also 1 project you cannot see.')).toBeVisible();
    await screen.unmount();
    const priority = await render(FieldPage, { field: await ensureField(1), ondeleted: vi.fn() });
    const list = priority.getByRole('list', { name: 'Projects using “Priority”' });
    await expect
      .element(list.getByRole('link', { name: 'Alpha' }))
      .toHaveAttribute('href', '/projects/ALPHA');
    await expect.element(list.getByRole('link', { name: 'Beta' })).toBeVisible();
  });

  it('FLD-011: a group field has no list of values of its own', async () => {
    setAccessForTests(SYSTEM);
    server();
    const screen = await render(FieldPage, { field: await ensureField(4), ondeleted: vi.fn() });
    await expect
      .element(screen.getByText(/A group field offers every group people can see/))
      .toBeVisible();
    await expect.element(screen.getByText('No project uses this field.')).toBeVisible();
  });

  it('BRD-022: lists the boards using a field and how they use it; deleting counts them', async () => {
    setAccessForTests(SYSTEM);
    server({
      1: {
        items: [
          { id: 31, name: 'Sprint board', column: true, estimation: true },
          { id: 32, name: 'Flow', lane: true, color: true, cards: true },
        ],
        others: 1,
      },
    });
    const screen = await render(FieldPage, { field: await ensureField(1), ondeleted: vi.fn() });
    const list = screen.getByRole('list', { name: 'Boards using “Priority”' });
    await expect
      .element(list.getByRole('link', { name: 'Sprint board' }))
      .toHaveAttribute('href', '/boards/31');
    await expect.element(list.getByRole('link', { name: 'Flow' })).toBeVisible();
    // How each board uses the field, in the board settings' own words.
    await expect.element(screen.getByText('columns, estimation')).toBeVisible();
    await expect.element(screen.getByText('lanes, card color, fields on cards')).toBeVisible();
    await expect.element(screen.getByText('1 more board, which you do not view.')).toBeVisible();
    if (isPhone()) expect(await smallTargets(screen.container)).toEqual([]);

    await screen.getByRole('button', { name: 'Delete field' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete “Priority”?' });
    await expect
      .element(
        dialog.getByText(
          'It is removed from 2 projects. Its values are erased in 120 issues. 3 boards stop using it. This cannot be undone.',
        ),
      )
      .toBeVisible();
    await screen.unmount();

    // A field no board uses says so.
    const severity = await render(FieldPage, { field: await ensureField(5), ondeleted: vi.fn() });
    await expect.element(severity.getByText('No board uses this field.')).toBeVisible();
  });

  it('FLD-023: deleting a field shows how many projects and issues lose it', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = server();
    const ondeleted = vi.fn();
    const screen = await render(FieldPage, { field: await ensureField(1), ondeleted });
    await screen.getByRole('button', { name: 'Delete field' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete “Priority”?' });
    await expect
      .element(
        dialog.getByText(
          'It is removed from 2 projects. Its values are erased in 120 issues. This cannot be undone.',
        ),
      )
      .toBeVisible();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Delete field' }).click();
    await vi.waitFor(() => {
      expect(ondeleted).toHaveBeenCalledOnce();
    });
    expect(writes(calls).at(-1)).toMatchObject({ method: 'DELETE', path: '/fields/1' });
    expect(toasts.items.at(-1)?.message).toBe('Field deleted');
  });

  it('FLD-023: a field no issue fills says so; Russian texts', async () => {
    setAccessForTests(SYSTEM);
    setLocale('ru');
    server();
    const screen = await render(FieldPage, { field: await ensureField(5), ondeleted: vi.fn() });
    await screen.getByRole('button', { name: 'Удалить поле' }).click();
    const dialog = page.getByRole('dialog', { name: 'Удалить поле «Severity»?' });
    await expect
      .element(
        dialog.getByText(
          'Поле будет убрано из 1\u00a0проекта. Ни в одной задаче оно не заполнено. Это нельзя отменить.',
        ),
      )
      .toBeVisible();
    if (isPhone()) expect(await smallTargets(dialog.element())).toEqual([]);
  });
});
