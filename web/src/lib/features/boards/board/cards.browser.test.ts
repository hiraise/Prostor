// Cards by their settings on a desktop and on a phone (step 1.8c-4; BRD-059…BRD-063, ISS-042):
// the «Карточка» tab, fields in full or by first letter, color stripes with accessible names,
// the lock and the file count, editing on the card, estimation sums.
import { userEvent } from 'vitest/browser';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { Board, BoardView, ProjectField } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';

import { expectAccessible } from '../../../../test/a11y.ts';
import { fakeApi, failure, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import SettingsPanel from '../settings/SettingsPanel.svelte';
import {
  ASSIGNEE,
  CARDS,
  ESTIMATION,
  MAJOR,
  MEMBER,
  PRIORITY,
  SESSION,
  STATE,
  board,
  issue,
  view,
} from '../test-data.ts';
import BoardPage from './BoardPage.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

function server(routes: Record<string, FakeRoute>, shown: BoardView = view()): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({ 'GET /boards/9/view': { body: shown }, ...routes });
}

/** The board with sums: 90 minutes in «Open» and on the board. */
function summed(extra: Partial<BoardView> = {}): BoardView {
  const shown = view(extra);
  return {
    ...shown,
    columns: shown.columns.map((one) => ({ ...one, estimate: one.column === 1 ? 90 : 0 })),
    totals: { ...shown.totals, estimate: 90 },
  };
}

function projectField(field: ProjectField['field']): ProjectField {
  return {
    aliases: [],
    can_be_empty: true,
    default_users: [],
    default_values: [],
    empty_label: '',
    field,
    field_default: true,
    private: false,
    required: false,
    set: null,
    shared_with: 1,
    version: 1,
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-059: card fields', () => {
  it('the «Карточка» tab adds, orders by first letter and removes fields', async () => {
    const calls = server({
      'GET /projects/1/fields': {
        body: { items: [STATE, PRIORITY, ASSIGNEE, ESTIMATION].map(projectField) },
      },
      'PATCH /boards/9': { body: board() },
    });
    const onchanged = vi.fn();
    const screen = await render(SettingsPanel, { open: true, board: board(), onchanged });
    const dialog = screen.getByRole('dialog', { name: 'Board settings' });
    await dialog.getByRole('tab', { name: 'Card' }).click();
    await expect.element(dialog.getByText('Priority', { exact: true })).toBeVisible();

    // The column field is not offered: its value is the column.
    const add = dialog.getByRole('combobox', { name: 'Add a field' });
    await expect.element(add).toBeEnabled();
    const offered = [...(add.element() as HTMLSelectElement).options].map((one) => one.text);
    expect(offered).toEqual(['Choose a field', 'Assignee', 'Estimation']);
    await add.selectOptions('Assignee');
    await dialog.getByRole('checkbox', { name: 'First letter only' }).click();
    await dialog.getByRole('button', { name: 'Remove Priority from cards' }).click();
    await expect.poll(() => onchanged.mock.calls.length).toBe(3);
    const sent = calls.filter((call) => call.method === 'PATCH').map((call) => call.body);
    expect(sent).toEqual([
      expect.objectContaining({
        cards: expect.objectContaining({ fields: [PRIORITY.id, ASSIGNEE.id] }) as unknown,
      }),
      expect.objectContaining({
        cards: expect.objectContaining({ initials: [PRIORITY.id] }) as unknown,
      }),
      expect.objectContaining({
        cards: expect.objectContaining({ fields: [], initials: [] }) as unknown,
      }),
    ]);
    await expectAccessible(screen.container);
  });

  it('a field shown by its first letter keeps the full name for screen readers', async () => {
    const shown = view({ board: board({ cards: { ...CARDS, initials: [PRIORITY.id] } }) });
    server({}, shown);
    const screen = await render(BoardPage, { board: shown.board });
    const card = screen.getByRole('article', { name: 'WEB-101' });
    const initial = card.getByTitle('Major');
    await expect.element(initial).toBeVisible();
    expect(initial.element().textContent).toBe('MMajor');
  });

  it('folds empty fields but keeps their editors one disclosure away', async () => {
    server({});
    const screen = await render(BoardPage, { board: board() });
    const card = screen.getByRole('article', { name: 'WEB-102' });
    expect(card.getByText(/Priority: not set/).elements()).toHaveLength(0);

    await card.getByRole('button', { name: '1 empty field' }).click();
    await expect.element(card.getByText(/Priority: not set/)).toBeVisible();
    await expect.element(card.getByRole('button', { name: /Priority of WEB-102/ })).toBeVisible();
    await card.getByRole('button', { name: 'Hide empty fields' }).click();
    await expect.element(card.getByText(/Priority: not set/)).not.toBeInTheDocument();
  });
});

describe('BRD-060: card colors', () => {
  it('by a field: only the stripe is visible, with the value named for screen readers', async () => {
    const cards: Board['cards'] = { ...CARDS, fields: [], color: 'field', color_field: PRIORITY };
    const shown = view({ board: board({ cards }) });
    server({}, shown);
    const screen = await render(BoardPage, { board: shown.board });
    const card = screen.getByRole('article', { name: 'WEB-101' });
    await expect.element(card.getByText('Color: Major')).toBeInTheDocument();
    expect(card.getByText('Major').elements()).toHaveLength(0);
    const element = card.element() as HTMLElement;
    expect(element.classList.contains('striped')).toBe(true);
    expect(element.style.getPropertyValue('--stripe')).toBe('var(--value-3-fg)');
    // A card without a colored value has no stripe.
    const plain = screen.getByRole('article', { name: 'WEB-102' }).element();
    expect(plain.classList.contains('striped')).toBe(false);
  });

  it('by project: the project’s color, named for screen readers', async () => {
    const cards: Board['cards'] = { ...CARDS, color: 'project' };
    const shown = view({
      board: board({ cards, projects: board().projects.map((one) => ({ ...one, color: 5 })) }),
    });
    server({}, shown);
    const screen = await render(BoardPage, { board: shown.board });
    const card = screen.getByRole('article', { name: 'WEB-102' });
    await expect.element(card).toBeVisible();
    expect((card.element() as HTMLElement).style.getPropertyValue('--stripe')).toBe(
      'var(--value-5-fg)',
    );
    await expect.element(card.getByText('Color: Web')).toBeInTheDocument();
  });
});

describe('BRD-059: card layout', () => {
  it('aligns an editable field chip with the start of the title', async () => {
    server({});
    const screen = await render(BoardPage, { board: board() });
    const card = screen.getByRole('article', { name: 'WEB-101' });
    await expect.element(card).toBeVisible();
    const title = card.element().querySelector('.title');
    if (!title) throw new Error('no card title');
    const chip = card.getByText('Major', { exact: true }).element();
    expect(chip.getBoundingClientRect().left).toBeCloseTo(title.getBoundingClientRect().left, 0);
  });
});

describe('BRD-062: estimation', () => {
  it('shows when a card has no estimate', async () => {
    server({});
    const screen = await render(BoardPage, { board: board() });
    const card = screen.getByRole('article', { name: 'WEB-102' });
    await expect.element(card.getByText('No estimate', { exact: true })).toBeVisible();
  });

  it('sums columns and the board, and an edit on a card moves the sums', async () => {
    const calls = server({ 'PUT /issues/101/fields/8': { body: {} } }, summed());
    const screen = await render(BoardPage, { board: board() });
    await expect.element(screen.getByText('Estimation in total: 1h 30m')).toBeVisible();
    const header = screen.getByRole('heading', { name: 'Open', level: 2 }).element().parentElement;
    expect(header?.textContent).toContain('1h 30m');

    const card = screen.getByRole('article', { name: 'WEB-101' });
    await card.getByRole('button', { name: /Estimation of WEB-101/ }).click();
    const input = screen.getByRole('textbox', { name: 'Estimation' });
    await expect.element(input).toHaveValue('1h 30m');
    await userEvent.fill(input, '1d 2h');
    await userEvent.keyboard('{Enter}');
    await expect.element(screen.getByText('Estimation in total: 1d 2h')).toBeVisible();
    expect(header?.textContent).toContain('1d 2h');
    const sent = calls.find((call) => call.method === 'PUT');
    expect(sent?.body).toEqual({ value: 600 });
  });
});

describe('BRD-063: editing on cards', () => {
  it('takes a tag off, and a refusal brings it back', async () => {
    let refuse = false;
    const shown = view({ profile: { ...view().profile, card_size: 2 } });
    server(
      {
        'DELETE /issues/101/tags/1': () => (refuse ? failure(403, 'forbidden') : { status: 204 }),
      },
      shown,
    );
    const screen = await render(BoardPage, { board: board() });
    const card = screen.getByRole('article', { name: 'WEB-101' });
    refuse = true;
    await card.getByRole('button', { name: 'Remove tag ui' }).click();
    await expect.element(card.getByRole('button', { name: 'Remove tag ui' })).toBeVisible();
    refuse = false;
    await card.getByRole('button', { name: 'Remove tag ui' }).click();
    await expect.element(card.getByRole('list', { name: 'Tags' })).not.toBeInTheDocument();
  });

  it('people who may not change the issue see values without editors', async () => {
    const shown = view();
    shown.issues = shown.issues.map((one) => ({ ...one, can_update: false }));
    server({}, shown);
    const screen = await render(BoardPage, { board: board() });
    const card = screen.getByRole('article', { name: 'WEB-101' });
    await expect.element(card.getByText('Major')).toBeVisible();
    expect(
      card.getByRole('button', { name: /(Priority|Estimation|Assignee) of WEB-101/ }).elements(),
    ).toHaveLength(0);
    // An empty field is left out for them.
    const other = screen.getByRole('article', { name: 'WEB-102' });
    expect(other.getByText(/not set/).elements()).toHaveLength(0);
  });

  it('a field that cannot be emptied offers no empty choice on the card (FLD-057)', async () => {
    const shown = view({ profile: { ...view().profile, card_size: 2 } });
    server(
      {
        [`GET /issues/101/fields/${String(PRIORITY.id)}/options`]: {
          body: {
            values: [{ ...MAJOR, released: false, release_date: null }],
            users: [],
            groups: [],
            more: false,
            can_add_value: false,
            set: 3,
          },
        },
      },
      shown,
    );
    const screen = await render(BoardPage, { board: board() });
    const card = screen.getByRole('article', { name: 'WEB-101' });
    await card.getByRole('button', { name: /Priority of WEB-101/ }).click();
    await expect.element(screen.getByRole('radio', { name: 'Major' })).toBeVisible();
    // The field cannot be emptied: the list holds its values only, no empty row.
    expect(screen.getByRole('radio').elements()).toHaveLength(1);
  });
});

describe('ISS-042, BRD-058: lock and files', () => {
  it('a restricted issue shows a lock, files show their count', async () => {
    const shown = view();
    shown.issues = [
      issue(101, 'Login form', { restricted: true, files: 2 }),
      ...shown.issues.slice(1),
    ];
    server({}, shown);
    const screen = await render(BoardPage, { board: board() });
    const card = screen.getByRole('article', { name: 'WEB-101' });
    await expect
      .element(card.getByRole('img', { name: 'Visible to some people only' }))
      .toBeVisible();
    await expect.element(card.getByText('2 files')).toBeVisible();
    const open = screen.getByRole('article', { name: 'WEB-102' });
    expect(open.getByRole('img', { name: 'Visible to some people only' }).elements()).toHaveLength(
      0,
    );
    await expectAccessible(screen.container);
  });
});
