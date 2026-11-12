// The link types of the instance on a desktop and on a phone (step 1.4c; LNK-018…LNK-021).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import { ensureLinkTypes } from './link-types.ts';
import LinkTypesPage from './LinkTypesPage.svelte';
import { builtinTypes, linkType, linkTypesServer, writes } from './test-server.ts';

const SYSTEM: AccessMap = {
  global: ['system.read', 'system.write', 'user.directory'],
  projects: [],
  capabilities: [],
};

const READER: AccessMap = {
  global: ['system.read', 'user.directory'],
  projects: [],
  capabilities: [],
};

async function typesList() {
  const list = await ensureLinkTypes();
  await list.refresh();
  return list;
}

function rowOf(name: string, list = 'Link types') {
  // The name starts the row; other rows may mention the same word further on.
  const start = new RegExp(`^${name}`);
  return page.getByRole('list', { name: list }).getByRole('listitem').filter({ hasText: start });
}

function names(list = 'Link types'): string[] {
  return page
    .getByRole('list', { name: list })
    .getByRole('listitem')
    .elements()
    .map((item) => item.querySelector('.name')?.textContent.trim() ?? '');
}

afterEach(async () => {
  // A refresh the page started last is shared here, so it ends on this test's server.
  await (await ensureLinkTypes()).refresh().catch(() => undefined);
  vi.unstubAllGlobals();
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
  store.entities.table('link_type').clear();
});

describe('LinkTypesPage', () => {
  it('LNK-018: lists the types with their sides, direction and use; locked ones offer no actions', async () => {
    setAccessForTests(SYSTEM);
    linkTypesServer({
      types: [
        ...builtinTypes(),
        linkType(5, 'Causes', 'causes', 'caused by', {
          deleted_at: '2026-10-22T09:00:00Z',
          usage: 2,
        }),
      ],
    });
    const screen = await render(LinkTypesPage, { types: await typesList() });

    expect(names()).toEqual(['Related', 'Dependency', 'Duplicate', 'Subtask']);
    await expect.element(rowOf('Related')).toMatchTextContent(/Both sides: “related to”/);
    await expect.element(rowOf('Related')).toMatchTextContent(/Undirected · 4 links/);
    await expect
      .element(rowOf('Dependency'))
      .toMatchTextContent(/Sides: “blocks” and “blocked by”/);
    await expect.element(rowOf('Dependency')).toMatchTextContent(/Directed · 12 links/);
    await expect.element(rowOf('Dependency').getByText('Built-in')).toBeVisible();
    await expect.element(rowOf('Duplicate')).toMatchTextContent(/Hierarchy · 1 link$/);
    await expect.element(rowOf('Subtask').getByText('Cannot be changed')).toBeVisible();
    await expect
      .element(screen.getByRole('button', { name: 'Actions for “Subtask”' }))
      .not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'Actions for “Duplicate”' }))
      .not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'Actions for “Dependency”' }))
      .toBeVisible();

    await expect
      .element(screen.getByRole('heading', { level: 2, name: 'Deleted types' }))
      .toBeVisible();
    expect(names('Deleted types')).toEqual(['Causes']);
    await expect.element(screen.getByRole('button', { name: 'Restore “Causes”' })).toBeVisible();

    if (isPhone()) {
      // Each type is a card of its own.
      const card = rowOf('Related').element();
      expect(getComputedStyle(card).borderTopStyle).toBe('solid');
      expect(getComputedStyle(card).borderBottomStyle).toBe('solid');
    }
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('LNK-019: creates a type; an undirected one has a single name; names taken are shown on their inputs', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = linkTypesServer();
    const screen = await render(LinkTypesPage, { types: await typesList() });

    await screen.getByRole('button', { name: 'Create link type' }).click();
    const dialog = page.getByRole('dialog', { name: 'New link type' });
    await expect.element(dialog.getByRole('radio', { name: 'Directed' })).toBeChecked();
    await dialog.getByRole('button', { name: 'Create type' }).click();
    await expect
      .element(dialog.getByLabelText(/^Name\s*\*?$/))
      .toHaveAccessibleDescription(/Fill in this field\./);
    expect(writes(calls)).toEqual([]);

    await dialog.getByRole('radio', { name: 'Undirected' }).click();
    await expect.element(dialog.getByLabelText(/^Target side/)).not.toBeInTheDocument();
    await expect.element(dialog.getByLabelText(/^Link name/)).toBeVisible();
    await dialog.getByRole('radio', { name: 'Directed' }).click();
    await expect.element(dialog.getByLabelText(/^Target side/)).toBeVisible();

    await dialog.getByLabelText(/^Name\s*\*?$/).fill('Causes');
    await dialog.getByLabelText(/^Source side/).fill('causes');
    await dialog.getByLabelText(/^Target side/).fill('Blocked by');
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Create type' }).click();
    await expect
      .element(dialog.getByLabelText(/^Target side/))
      .toHaveAccessibleDescription(/Another link type uses these words\./);
    await expect.element(dialog.getByLabelText(/^Target side/)).toHaveFocus();

    await dialog.getByLabelText(/^Target side/).fill('caused by');
    await dialog.getByRole('button', { name: 'Create type' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'POST',
      path: '/link-types',
      body: {
        name: 'Causes',
        source_name: 'causes',
        target_name: 'caused by',
        direction: 'directed',
      },
    });
    expect(toasts.items.at(-1)?.message).toBe('Link type created');
    await vi.waitFor(() => {
      expect(names().at(-1)).toBe('Causes');
    });
    await expect.element(rowOf('Causes')).toMatchTextContent(/Directed · No links/);

    await screen.getByRole('button', { name: 'Create link type' }).click();
    const again = page.getByRole('dialog', { name: 'New link type' });
    await again.getByLabelText(/^Name\s*\*?$/).fill('Mentions');
    await again.getByRole('radio', { name: 'Undirected' }).click();
    await again.getByLabelText(/^Link name/).fill('mentions');
    await again.getByRole('button', { name: 'Create type' }).click();
    await vi.waitFor(() => {
      expect(writes(calls).at(-1)?.body).toEqual({
        name: 'Mentions',
        source_name: 'mentions',
        direction: 'undirected',
      });
    });
  });

  it('LNK-020: renames a type and drops its direction with warnings; a change made meanwhile is reported', async () => {
    setAccessForTests(SYSTEM);
    let stale = false;
    const { calls, types } = linkTypesServer({
      override: {
        'PATCH /link-types/2': () => {
          if (!stale) return undefined;
          stale = false;
          const dependency = types.get(2);
          if (dependency) {
            dependency.name = 'Blocking';
            dependency.direction = 'hierarchy';
            dependency.target_name = 'held by';
            dependency.version += 1;
          }
          return { status: 409, body: { code: 'conflict', message: 'conflict' } };
        },
      },
    });
    const screen = await render(LinkTypesPage, { types: await typesList() });

    await screen.getByRole('button', { name: 'Actions for “Dependency”' }).click();
    await page.getByRole('menuitem', { name: 'Edit' }).click();
    const dialog = page.getByRole('dialog', { name: 'Edit “Dependency”' });
    await expect.element(dialog.getByLabelText(/^Source side/)).toHaveValue('blocks');
    await expect.element(dialog.getByLabelText(/^Target side/)).toHaveValue('blocked by');
    await expect.element(dialog.getByText(/New words apply to every link/)).not.toBeInTheDocument();

    await dialog.getByLabelText(/^Source side/).fill('holds up');
    await expect.element(dialog.getByText(/New words apply to every link/)).toBeVisible();
    await dialog.getByRole('radio', { name: 'Undirected' }).click();
    await expect.element(dialog.getByLabelText(/^Target side/)).not.toBeInTheDocument();
    await expect.element(dialog.getByText(/the target side “blocked by” is dropped/)).toBeVisible();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(writes(calls).at(-1)).toEqual(
      expect.objectContaining({
        method: 'PATCH',
        path: '/link-types/2',
        body: { source_name: 'holds up', direction: 'undirected', version: 1 },
      }),
    );
    await expect.element(rowOf('Dependency')).toMatchTextContent(/Both sides: “holds up”/);
    expect(toasts.items.at(-1)?.message).toBe('Link type saved');

    // Back to directed: the target side is needed again. Meanwhile someone else renames the type
    // and makes it a hierarchy: the inputs this person did not touch take their values.
    stale = true;
    await screen.getByRole('button', { name: 'Actions for “Dependency”' }).click();
    await page.getByRole('menuitem', { name: 'Edit' }).click();
    const back = page.getByRole('dialog');
    await back.getByRole('radio', { name: 'Directed' }).click();
    await back.getByRole('button', { name: 'Save' }).click();
    await expect
      .element(back.getByLabelText(/^Target side/))
      .toHaveAccessibleDescription(/Fill in this field\./);
    await back.getByLabelText(/^Target side/).fill('held up by');
    await back.getByRole('button', { name: 'Save' }).click();
    await expect.element(back.getByText(/Someone else changed this type meanwhile/)).toBeVisible();
    await expect.element(back.getByLabelText(/^Name\s*\*?$/)).toHaveValue('Blocking');
    await expect.element(back.getByRole('radio', { name: 'Directed' })).toBeChecked();
    await expect.element(back.getByLabelText(/^Target side/)).toHaveValue('held up by');
    await expect.element(rowOf('Blocking')).toMatchTextContent(/Hierarchy/);
    await back.getByRole('button', { name: 'Save' }).click();
    await expect.element(back).not.toBeInTheDocument();
    expect(writes(calls).at(-1)?.body).toEqual({
      target_name: 'held up by',
      direction: 'directed',
      version: 3,
    });
    await expect
      .element(rowOf('Blocking'))
      .toMatchTextContent(/Sides: “holds up” and “held up by”/);
    await expect.element(rowOf('Blocking')).toMatchTextContent(/Directed/);
  });

  it('LNK-021: deleting asks with the number of links; deleted types are restored', async () => {
    setAccessForTests(SYSTEM);
    const { calls } = linkTypesServer();
    const screen = await render(LinkTypesPage, { types: await typesList() });

    await screen.getByRole('button', { name: 'Actions for “Dependency”' }).click();
    await page.getByRole('menuitem', { name: 'Delete' }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete “Dependency”?' });
    await expect
      .element(dialog.getByText('12 links of this type are hidden from issues.'))
      .toBeVisible();
    await expect
      .element(dialog.getByText(/restore it there to bring its links back/))
      .toBeVisible();
    await expectAccessible(dialog.element());
    if (isPhone()) expect(smallTargets(dialog.element())).toEqual([]);
    await dialog.getByRole('button', { name: 'Delete type' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(writes(calls).at(-1)).toMatchObject({ method: 'DELETE', path: '/link-types/2' });
    expect(toasts.items.at(-1)?.message).toBe('Link type deleted');
    await vi.waitFor(() => {
      expect(names('Deleted types')).toEqual(['Dependency']);
    });
    expect(names()).toEqual(['Related', 'Duplicate', 'Subtask']);

    await screen.getByRole('button', { name: 'Restore “Dependency”' }).click();
    await vi.waitFor(() => {
      expect(names()).toEqual(['Related', 'Dependency', 'Duplicate', 'Subtask']);
    });
    expect(writes(calls).at(-1)).toMatchObject({
      method: 'POST',
      path: '/link-types/2/restore',
    });
    expect(toasts.items.at(-1)?.message).toBe('Link type restored');
    await expect
      .element(screen.getByRole('heading', { name: 'Deleted types' }))
      .not.toBeInTheDocument();
  });

  it('LNK-021: a type whose name was taken meanwhile is not restored, and says why', async () => {
    setAccessForTests(SYSTEM);
    linkTypesServer({
      types: [
        ...builtinTypes(),
        linkType(5, 'Causes', 'causes', 'caused by', { deleted_at: '2026-10-22T09:00:00Z' }),
        linkType(6, 'Causes', 'leads to', 'follows'),
      ],
    });
    const screen = await render(LinkTypesPage, { types: await typesList() });
    await screen.getByRole('button', { name: 'Restore “Causes”' }).click();
    await vi.waitFor(() => {
      expect(toasts.items.at(-1)?.message).toBe(
        'Another type uses one of its names now. Rename that type first, then restore this one.',
      );
    });
    expect(names('Deleted types')).toEqual(['Causes']);
  });

  it('LNK-018, LNK-019, LNK-021: Russian texts', async () => {
    setAccessForTests(SYSTEM);
    setLocale('ru');
    linkTypesServer({
      types: [
        linkType(1, 'Связь', 'связана с', null, { builtin: 'related', usage: 3 }),
        linkType(2, 'Зависимость', 'блокирует', 'заблокирована', {
          builtin: 'dependency',
          usage: 5,
        }),
        linkType(4, 'Подзадача', 'родитель для', 'подзадача для', {
          builtin: 'subtask',
          direction: 'hierarchy',
          locked: true,
          usage: 21,
        }),
      ],
    });
    const screen = await render(LinkTypesPage, { types: await typesList() });
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Типы связей');
    await expect
      .element(rowOf('Связь', 'Типы связей'))
      .toMatchTextContent(/Обе стороны: «связана с»/);
    await expect
      .element(rowOf('Связь', 'Типы связей'))
      .toMatchTextContent(/Без направления · 3 связи/);
    await expect
      .element(rowOf('Зависимость', 'Типы связей'))
      .toMatchTextContent(/Стороны: «блокирует» и «заблокирована»/);
    await expect
      .element(rowOf('Зависимость', 'Типы связей'))
      .toMatchTextContent(/Направленная · 5 связей/);
    await expect
      .element(rowOf('Подзадача', 'Типы связей'))
      .toMatchTextContent(/Нельзя изменить.*Иерархия · 21 связь/);

    await screen.getByRole('button', { name: 'Создать тип связи' }).click();
    const create = page.getByRole('dialog', { name: 'Новый тип связи' });
    await expect.element(create.getByRole('radio', { name: 'Без направления' })).toBeVisible();
    await expect.element(create.getByRole('radio', { name: 'Иерархия' })).toBeVisible();
    await expect.element(create.getByLabelText(/^Обратное название/)).toBeVisible();
    await create.getByRole('button', { name: 'Отмена' }).click();

    await screen.getByRole('button', { name: 'Действия с «Зависимость»' }).click();
    await page.getByRole('menuitem', { name: 'Удалить' }).click();
    const dialog = page.getByRole('dialog', { name: 'Удалить тип связи «Зависимость»?' });
    await expect
      .element(dialog.getByText('5 связей этого типа будут скрыты в задачах.'))
      .toBeVisible();
    await dialog.getByRole('button', { name: 'Удалить тип' }).click();
    await expect
      .element(screen.getByRole('heading', { level: 2, name: 'Удалённые типы' }))
      .toBeVisible();
    await expect
      .element(screen.getByRole('button', { name: 'Восстановить «Зависимость»' }))
      .toBeVisible();
  });

  it('LNK-018: holders of system.read without system.write see the types read-only', async () => {
    setAccessForTests(READER);
    linkTypesServer({
      types: [
        ...builtinTypes(),
        linkType(5, 'Causes', 'causes', 'caused by', { deleted_at: '2026-10-22T09:00:00Z' }),
      ],
    });
    const screen = await render(LinkTypesPage, { types: await typesList() });
    await expect
      .element(screen.getByText('Only system administrators can change link types.'))
      .toBeVisible();
    await expect.element(rowOf('Dependency')).toBeVisible();
    await expect
      .element(screen.getByRole('button', { name: 'Create link type' }))
      .not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'Actions for “Dependency”' }))
      .not.toBeInTheDocument();
    expect(names('Deleted types')).toEqual(['Causes']);
    await expect
      .element(screen.getByRole('button', { name: 'Restore “Causes”' }))
      .not.toBeInTheDocument();
    await expectAccessible(screen.container);
  });

  it('LNK-018: people without system.read are told whom to ask', async () => {
    setAccessForTests({ global: ['user.directory'], projects: [], capabilities: [] });
    linkTypesServer({ limited: true });
    const screen = await render(LinkTypesPage, { types: null });
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Only administrators can open this');
  });
});
