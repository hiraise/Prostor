// The value set editor and the value sets pages on a desktop and on a phone (step 1.2; FLD-008,
// FLD-010, FLD-012…FLD-014, FLD-025, FLD-028, FLD-029, FLD-032, FLD-037, FLD-039, FLD-040,
// FLD-043…FLD-049).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import { failure } from '../../../test/fake-api.ts';
import { ensureValueSet, ensureValueSets } from './fields.ts';
import {
  attachment,
  fieldsServer,
  storedField,
  storedSet,
  value,
  writes,
  type StoredSet,
} from './test-server.ts';
import ValueSetPage from './ValueSetPage.svelte';
import ValueSetsPage from './ValueSetsPage.svelte';

const SYSTEM: AccessMap = {
  global: ['system.read', 'system.write', 'user.directory'],
  projects: [],
  capabilities: [],
};

async function setPage(id: number) {
  const handle = await ensureValueSet(id);
  return render(ValueSetPage, { set: handle, ondeleted: vi.fn() });
}

function valueNames(name: string): string[] {
  return page
    .getByRole('list', { name: `Values of “${name}”` })
    .getByRole('listitem')
    .elements()
    .map((item) => item.querySelector('.chip')?.textContent.trim() ?? '');
}

async function valueAction(name: string, action: string): Promise<void> {
  await page.getByRole('button', { name: `Actions for “${name}”` }).click();
  await page.getByRole('menuitem', { name: action }).click();
}

function serverWith(sets: StoredSet[], override?: Parameters<typeof fieldsServer>[0]) {
  return fieldsServer({
    sets,
    fields: [storedField(1, 'Priority', 'enum', { setId: 10 })],
    projects: { 1: [attachment(1, { setId: 10 })] },
    ...override,
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
  store.entities.table('value_set').clear();
});

describe('the value set editor', () => {
  it('FLD-008, FLD-032, FLD-043, FLD-044: adds a value with a description and a color, and renames one inline', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([
      storedSet(10, 'Priorities', 'enum', [value(100, 'High', { color: 1 }), value(101, 'Low')]),
    ]);
    const screen = await setPage(10);
    await expect.element(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Priorities');
    await expect.element(screen.getByText('Type: Choice')).toBeVisible();
    await expect.element(screen.getByText('Used in: Alpha.')).toBeVisible();
    await expect.element(screen.getByText('Default set of 1 field')).toBeVisible();
    expect(valueNames('Priorities')).toEqual(['High', 'Low']);
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await valueAction('High', 'Add value after it');
    const dialog = page.getByRole('dialog', { name: 'New value' });
    await expect.element(dialog.getByText('It goes right after “High”.')).toBeVisible();
    await dialog.getByLabelText(/^Name/).fill('Normal');
    await dialog.getByLabelText('Description').fill('Most issues');
    await dialog.getByRole('radio', { name: 'Blue' }).click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Add value' }).click();
    await vi.waitFor(() => {
      expect(valueNames('Priorities')).toEqual(['High', 'Normal', 'Low']);
    });
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'POST',
      path: '/value-sets/10/values',
      body: { name: 'Normal', description: 'Most issues', color: 8, after: 100 },
    });
    await expect.element(screen.getByText('Most issues')).toBeVisible();
    expect(toasts.items.at(-1)?.message).toBe('Value added');

    await valueAction('Low', 'Rename');
    const input = screen.getByRole('textbox', { name: 'New name for “Low”' });
    await expect.element(input).toHaveFocus();
    await input.fill('High');
    await userEvent.keyboard('{Enter}');
    await expect.element(screen.getByText('The set has a value with this name.')).toBeVisible();
    await expect.element(input).toHaveAttribute('aria-invalid', 'true');
    await input.fill('Minor');
    await userEvent.keyboard('{Enter}');
    await vi.waitFor(() => {
      expect(valueNames('Priorities')).toEqual(['High', 'Normal', 'Minor']);
    });
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'PATCH',
      path: '/value-sets/10/values/101',
      body: { name: 'Minor', version: 1 },
    });
    await expect.element(screen.getByRole('button', { name: 'Actions for “Minor”' })).toHaveFocus();
  });

  it('FLD-028: shows the limits the server reports', async () => {
    setAccessForTests(SYSTEM);
    serverWith([storedSet(10, 'Priorities', 'enum', [value(100, 'High')])], {
      override: {
        'POST /value-sets/10/values': () =>
          failure(400, 'invalid_request', [{ path: 'name', code: 'too_long', message: '' }]),
      },
    });
    const screen = await setPage(10);
    await screen.getByRole('button', { name: 'Add value' }).click();
    const dialog = page.getByRole('dialog', { name: 'New value' });
    await expect.element(dialog.getByText('It goes at the end of the list.')).toBeVisible();
    await dialog.getByLabelText(/^Name/).fill('One more');
    await dialog.getByRole('button', { name: 'Add value' }).click();
    await expect
      .element(dialog.getByText('The set holds the most values it can: 2,000.'))
      .toBeVisible();
    await expect.element(dialog.getByLabelText(/^Name/)).toHaveAttribute('aria-invalid', 'true');
  });

  it('FLD-037: moves values with Move up and down, and by dragging on a desktop', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([
      storedSet(10, 'Priorities', 'enum', [
        value(100, 'High'),
        value(101, 'Normal'),
        value(102, 'Low'),
      ]),
    ]);
    const screen = await setPage(10);
    await expect.element(screen.getByText('High')).toBeVisible();

    await valueAction('Low', 'Move up');
    await vi.waitFor(() => {
      expect(valueNames('Priorities')).toEqual(['High', 'Low', 'Normal']);
    });
    expect(writes(calls).at(-1)).toMatchObject({
      path: '/value-sets/10/values/102/move',
      body: { after: 100 },
    });
    await valueAction('High', 'Move down');
    await vi.waitFor(() => {
      expect(valueNames('Priorities')).toEqual(['Low', 'High', 'Normal']);
    });
    expect(writes(calls).at(-1)?.body).toEqual({ after: 102 });

    if (!isPhone()) {
      const grips = screen.container.querySelectorAll<HTMLElement>('.grip');
      const rows = screen.container.querySelectorAll<HTMLElement>('li.row');
      const normalGrip = grips[2];
      const lowRow = rows[0];
      if (!normalGrip || !lowRow) throw new Error('rows are missing');
      await userEvent.dragAndDrop(normalGrip, lowRow, { targetPosition: { x: 40, y: 3 } });
      await vi.waitFor(() => {
        expect(valueNames('Priorities')).toEqual(['Normal', 'Low', 'High']);
      });
      expect(writes(calls).at(-1)).toMatchObject({
        path: '/value-sets/10/values/101/move',
        body: { after: null },
      });
    }
  });

  it('FLD-045, FLD-046, FLD-009: marks states resolved in the list, archives and restores values, and hides archived ones', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([
      storedSet(12, 'States', 'state', [
        value(120, 'Open'),
        value(121, 'Fixed', { resolved: true }),
      ]),
    ]);
    const screen = await setPage(12);
    const resolvedOpen = screen.getByRole('checkbox', { name: 'Resolved: Open' });
    await expect.element(resolvedOpen).not.toBeChecked();
    await expect.element(screen.getByRole('checkbox', { name: 'Resolved: Fixed' })).toBeChecked();
    await resolvedOpen.click();
    await expect.element(resolvedOpen).toBeChecked();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)).toMatchObject({
        method: 'PATCH',
        path: '/value-sets/12/values/120',
        body: { resolved: true, version: 1 },
      });
    });

    await valueAction('Fixed', 'Archive');
    const fixedRow = screen.getByRole('listitem').filter({ hasText: 'Fixed' });
    await expect.element(fixedRow.getByText('Archived', { exact: true })).toBeVisible();
    expect(toasts.items.at(-1)?.message).toBe('“Fixed” archived');
    await screen.getByRole('checkbox', { name: 'Show archived values' }).click();
    await vi.waitFor(() => {
      expect(valueNames('States')).toEqual(['Open']);
    });
    await screen.getByRole('checkbox', { name: 'Show archived values' }).click();
    await valueAction('Fixed', 'Restore');
    await expect.element(fixedRow.getByText('Archived', { exact: true })).not.toBeInTheDocument();
    expect(writes(calls).at(-1)?.body).toEqual({ archived: false, version: 2 });
    await expectAccessible(screen.container);
  });

  it('FLD-047, FLD-013: versions are released in the list and carry release and start dates', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([
      storedSet(13, 'Versions', 'version', [value(130, '1.0'), value(131, '2.0')]),
    ]);
    const screen = await setPage(13);
    await screen.getByRole('checkbox', { name: 'Released: 1.0' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({ released: true, version: 1 });
    });
    await expect
      .element(screen.getByRole('checkbox', { name: 'Show released versions' }))
      .toBeVisible();

    await valueAction('2.0', 'Edit');
    const dialog = page.getByRole('dialog', { name: 'Edit “2.0”' });
    await dialog.getByLabelText('Release date').fill('2026-12-01');
    await dialog.getByLabelText('Start date').fill('2026-11-01');
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Save changes' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({
        release_date: '2026-12-01',
        start_date: '2026-11-01',
        version: 1,
      });
    });
    await expect.element(screen.getByText(/^Release:/)).toBeVisible();
    await expect.element(screen.getByText(/^Start:/)).toBeVisible();

    await screen.getByRole('checkbox', { name: 'Show released versions' }).click();
    await vi.waitFor(() => {
      expect(valueNames('Versions')).toEqual(['2.0']);
    });
  });

  it('FLD-048, FLD-014: builds carry a build date', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([storedSet(16, 'Builds', 'build', [value(160, '512')])]);
    const screen = await setPage(16);
    await valueAction('512', 'Edit');
    const dialog = page.getByRole('dialog', { name: 'Edit “512”' });
    await expect
      .element(dialog.getByRole('checkbox', { name: 'Released' }))
      .not.toBeInTheDocument();
    await dialog.getByLabelText('Build date').fill('2026-10-02');
    await dialog.getByRole('button', { name: 'Save changes' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({ assembled_on: '2026-10-02', version: 1 });
    });
    await expect.element(screen.getByText(/^Built:/)).toBeVisible();
  });

  it('FLD-049, FLD-012: values with owners take an owner from the people search', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([storedSet(14, 'Components', 'owned', [value(140, 'Backend')])]);
    const screen = await setPage(14);
    await valueAction('Backend', 'Edit');
    const dialog = page.getByRole('dialog', { name: 'Edit “Backend”' });
    await expect.element(dialog.getByText('No owner')).toBeVisible();
    await dialog.getByRole('button', { name: 'Choose owner' }).click();
    const picker = page.getByRole('dialog', { name: 'Choose owner' });
    await picker.getByRole('radio', { name: /Cleo Park/ }).click();
    await picker.getByRole('button', { name: 'Choose' }).click();
    await expect.element(dialog.getByText('Cleo Park')).toBeVisible();
    await dialog.getByRole('button', { name: 'Save changes' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({ owner_id: 9, version: 1 });
    });
    await expect.element(screen.getByText('Owner:')).toBeVisible();
    await expect.element(screen.getByRole('listitem').getByText('Cleo Park')).toBeVisible();
  });

  it('FLD-032: deletes a value after a confirmation that warns about issues', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([
      storedSet(10, 'Priorities', 'enum', [value(100, 'High'), value(101, 'Low')]),
    ]);
    const screen = await setPage(10);
    await valueAction('Low', 'Delete');
    const dialog = page.getByRole('dialog', { name: 'Delete “Low”?' });
    await expect.element(dialog.getByText(/issues that hold it lose it/)).toBeVisible();
    expect(dialog.getByRole('combobox').elements()).toEqual([]);
    await dialog.getByRole('button', { name: 'Delete value' }).click();
    await vi.waitFor(() => {
      expect(valueNames('Priorities')).toEqual(['High']);
    });
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'DELETE',
      path: '/value-sets/10/values/101',
    });
    await expect.element(screen.getByText('High')).toBeVisible();
  });

  it('FLD-032: deleting a value issues hold counts them and puts the chosen value instead', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith(
      [
        storedSet(10, 'Priorities', 'enum', [
          value(100, 'High'),
          value(101, 'Low'),
          value(102, 'Old', { archived: true }),
          value(103, 'Normal'),
        ]),
      ],
      { issues: { values: { 101: 12 } } },
    );
    await setPage(10);
    await valueAction('Low', 'Delete');
    const dialog = page.getByRole('dialog', { name: 'Delete “Low”?' });
    await expect
      .element(
        dialog.getByText(
          '12 issues hold this value. Choose what they get instead, or leave the field empty there.',
        ),
      )
      .toBeVisible();
    const select = dialog.getByRole('combobox', { name: 'Put instead' });
    // Clearing is the default; archived values and the deleted one are not offered.
    await expect.element(select).toHaveValue('');
    expect(
      [...(select.element() as HTMLSelectElement).options].map((option) => option.textContent),
    ).toEqual(['Don’t replace — clear the field', 'High', 'Normal']);
    await expect.element(dialog.getByText('Issues are updated in the background.')).toBeVisible();
    await expectAccessible(dialog.element());
    if (isPhone()) expect(smallTargets(dialog.element())).toEqual([]);

    await select.selectOptions('Normal');
    await dialog.getByRole('button', { name: 'Delete value' }).click();
    await vi.waitFor(() => {
      expect(valueNames('Priorities')).toEqual(['High', 'Old', 'Normal']);
    });
    const deleted = writes(calls).at(-1);
    expect(deleted).toMatchObject({ method: 'DELETE', path: '/value-sets/10/values/101' });
    expect(deleted?.query.get('replacement')).toBe('103');
  });

  it('FLD-032: deleting a used value without a replacement clears it; Russian texts', async () => {
    setAccessForTests(SYSTEM);
    setLocale('ru');
    const { calls } = serverWith(
      [storedSet(10, 'Priorities', 'enum', [value(100, 'High'), value(101, 'Low')])],
      { issues: { values: { 101: 1 } } },
    );
    await setPage(10);
    await page.getByRole('button', { name: 'Действия со значением «Low»' }).click();
    await page.getByRole('menuitem', { name: 'Удалить' }).click();
    const dialog = page.getByRole('dialog', { name: 'Удалить «Low»?' });
    await expect
      .element(
        dialog.getByText(
          'Это значение выбрано в 1\u00a0задаче. Выберите, что поставить вместо него, или оставьте поле пустым.',
        ),
      )
      .toBeVisible();
    await expect
      .element(dialog.getByRole('combobox', { name: 'Поставить вместо него' }))
      .toHaveDisplayValue('Не заменять — очистить поле');
    await dialog.getByRole('button', { name: 'Удалить значение' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)).toMatchObject({
        method: 'DELETE',
        path: '/value-sets/10/values/101',
      });
    });
    expect(writes(calls).at(-1)?.query.has('replacement')).toBe(false);
  });

  it('FLD-039, FLD-040, FLD-010: a user set lists people and groups by name, a team by its project, and changes them', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([
      storedSet(11, 'Alpha people', 'user', [], { users: [9, 7], groups: [32, 31] }),
    ]);
    const screen = await setPage(11);
    const people = screen.getByRole('list', { name: 'People of “Alpha people”' });
    await expect.element(people).toBeVisible();
    expect(
      people
        .getByRole('listitem')
        .elements()
        .map((item) => item.textContent.trim()),
    ).toEqual([expect.stringContaining('Ann Lee'), expect.stringContaining('Cleo Park')]);
    const groups = screen.getByRole('list', { name: 'Groups of “Alpha people”' });
    expect(
      groups
        .getByRole('listitem')
        .elements()
        .map((item) => item.textContent.trim()),
    ).toEqual(['Alpha team', 'Backend']);
    await expect.element(screen.getByRole('button', { name: 'Add value' })).not.toBeInTheDocument();
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await screen.getByRole('button', { name: 'Add people' }).click();
    const picker = page.getByRole('dialog', { name: 'Add people to “Alpha people”' });
    await picker.getByRole('checkbox', { name: /Bob Stone/ }).click();
    await picker.getByRole('button', { name: 'Add people' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)).toMatchObject({
        path: '/value-sets/11/members',
        body: { users: [8] },
      });
    });
    expect(toasts.items.at(-1)?.message).toBe('1 person added');

    await screen.getByRole('button', { name: 'Add groups' }).click();
    const groupPicker = page.getByRole('dialog', { name: 'Add groups to “Alpha people”' });
    await expect.element(groupPicker.getByRole('checkbox', { name: /Backend/ })).toBeDisabled();
    await groupPicker.getByRole('checkbox', { name: /Designers/ }).click();
    await groupPicker.getByRole('button', { name: 'Add groups' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({ groups: [30] });
    });

    await screen.getByRole('button', { name: 'Remove Alpha team from the set' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)).toMatchObject({
        path: '/value-sets/11/members/remove',
        body: { groups: [31] },
      });
    });
    await expect.element(groups.getByText('Alpha team')).not.toBeInTheDocument();
    expect(toasts.items.at(-1)?.message).toBe('Alpha team removed from the set');
  });
});

describe('the value sets pages', () => {
  it('FLD-025, FLD-029: lists sets with their type and use, filters by type and name, and creates one', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([
      storedSet(10, 'Priorities', 'enum', [value(100, 'High')]),
      storedSet(12, 'States', 'state'),
      storedSet(17, 'Spare', 'enum'),
    ]);
    const list = await ensureValueSets({ q: '', kind: '' });
    await list.refresh();
    const oncreated = vi.fn();
    const screen = await render(ValueSetsPage, { sets: list, oncreated });
    await expect
      .element(screen.getByRole('link', { name: 'Priorities' }))
      .toHaveAccessibleDescription('Choice · Used in 1 project · Default set of 1 field');
    await expect
      .element(screen.getByRole('link', { name: 'Spare' }))
      .toHaveAccessibleDescription('Choice · Not used in any project');
    await expect
      .element(screen.getByRole('link', { name: 'Priorities' }))
      .toHaveAttribute('href', '/admin/value-sets/10');
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);

    await screen.getByLabelText('Type').first().selectOptions('state');
    await expect.element(screen.getByRole('link', { name: 'Priorities' })).not.toBeInTheDocument();
    await expect.element(screen.getByRole('link', { name: 'States' })).toBeVisible();
    await screen.getByLabelText('Type').first().selectOptions('');
    await screen.getByLabelText('Find sets').fill('nothing');
    await expect.element(screen.getByText('No sets found')).toBeVisible();

    await screen.getByRole('button', { name: 'Create set' }).click();
    const dialog = page.getByRole('dialog', { name: 'New value set' });
    await dialog.getByLabelText(/^Name/).fill('Priorities');
    await dialog.getByRole('button', { name: 'Create set' }).click();
    await expect.element(dialog.getByText('Another set of this type has this name.')).toBeVisible();
    await dialog.getByLabelText(/^Name/).fill('Severities');
    await dialog.getByLabelText('Type').selectOptions('state');
    await dialog.getByRole('button', { name: 'Create set' }).click();
    await vi.waitFor(() => {
      expect(oncreated).toHaveBeenCalledOnce();
    });
    expect(writes(calls).at(-1)?.body).toEqual({ name: 'Severities', kind: 'state' });
    expect(toasts.items.at(-1)?.message).toBe('Set created');
  });

  it('FLD-029: renames a set and deletes it once nothing uses it', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = serverWith([
      storedSet(10, 'Priorities', 'enum'),
      storedSet(17, 'Spare', 'enum'),
    ]);
    const used = await setPage(10);
    await expect
      .element(used.getByText('A set that fields or projects use cannot be deleted.'))
      .toBeVisible();
    await expect.element(used.getByRole('button', { name: 'Delete set' })).not.toBeInTheDocument();
    await used.unmount();

    const ondeleted = vi.fn();
    const screen = await render(ValueSetPage, { set: await ensureValueSet(17), ondeleted });
    await screen.getByRole('button', { name: 'Rename' }).click();
    const rename = page.getByRole('dialog', { name: 'Rename the set' });
    await rename.getByLabelText(/^Name/).fill('Extras');
    await rename.getByRole('button', { name: 'Save' }).click();
    await expect.element(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Extras');
    expect(writes(calls).at(-1)?.body).toEqual({ name: 'Extras', version: 1 });

    await screen.getByRole('button', { name: 'Delete set' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete “Extras”?' });
    await dialog.getByRole('button', { name: 'Delete set' }).click();
    await vi.waitFor(() => {
      expect(ondeleted).toHaveBeenCalledOnce();
    });
    expect(writes(calls).at(-1)).toMatchObject({ method: 'DELETE', path: '/value-sets/17' });
  });

  it('FLD-027: people who may not change a set see it read-only', async () => {
    setAccessForTests({
      global: [],
      projects: [{ project_id: 1, permissions: ['project.update'] }],
      capabilities: [],
    });
    serverWith([storedSet(10, 'Priorities', 'enum', [value(100, 'High')], { locked: true })]);
    const screen = await setPage(10);
    await expect
      .element(screen.getByText(/Only someone who configures every project using this set/))
      .toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Rename' })).not.toBeInTheDocument();
    await expect.element(screen.getByRole('button', { name: 'Add value' })).not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'Actions for “High”' }))
      .not.toBeInTheDocument();
    await expectAccessible(screen.container);
  });
});
