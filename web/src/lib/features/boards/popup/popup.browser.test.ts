// Creating on the board, the issue popup and the board's health (step 1.8c-5; BRD-023, BRD-054,
// BRD-076, BRD-077, BRD-079, BRD-080), on a desktop and on a phone.
import { userEvent } from 'vitest/browser';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Board, BoardView } from '#lib/api/schema.gen.ts';
import { issue as fullIssue, issuePage } from '#lib/features/issues/test-data.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';

import { expectAccessible } from '../../../../test/a11y.ts';
import { fakeApi, failure, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import BoardPage from '../board/BoardPage.svelte';
import { NO_LANES, SESSION, board, issue, view } from '../test-data.ts';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

/** Reads, updates and creates issues in project 1. */
const CREATOR: AccessMap = {
  global: [],
  projects: [
    {
      project_id: 1,
      permissions: ['project.basic', 'issue.read', 'issue.update', 'issue.create'],
    },
  ],
  capabilities: [],
};

function server(
  routes: Record<string, FakeRoute>,
  shown: BoardView = view(),
  access: AccessMap = CREATOR,
): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(access);
  return fakeApi({
    'GET /boards/9/view': { body: shown },
    'PATCH /users/me/preferences': { body: {} },
    ...routes,
  });
}

const LOGIN = fullIssue({ id: 101, readable_id: 'WEB-101', summary: 'Login form' });
const DRAFT = fullIssue({
  id: 900,
  readable_id: null,
  number: null,
  draft: true,
  summary: '',
  description: '',
  description_html: '',
  fields: [],
});

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-079: the issue popup', () => {
  it('a click on a card opens the issue over the board; Esc closes it', async () => {
    server({ 'GET /issues/101/page': { body: issuePage(LOGIN) } });
    const screen = await render(BoardPage, { board: board() });
    await screen.getByRole('link', { name: 'WEB-101' }).click();
    const popup = screen.getByRole('dialog', { name: 'WEB-101 Login form' });
    await expect.element(popup).toBeVisible();
    await expect
      .element(popup.getByRole('heading', { name: 'Login form', level: 1 }))
      .toBeVisible();
    await expect.element(popup.getByRole('button', { name: 'Open the page' })).toBeVisible();
    // No breadcrumbs inside the board.
    await expect
      .element(popup.getByRole('navigation', { name: 'Breadcrumbs' }))
      .not.toBeInTheDocument();
    await expectAccessible(popup.element());
    await userEvent.keyboard('{Escape}');
    await expect.element(popup).not.toBeInTheDocument();
  });

  it('Ctrl+click keeps the link to the page', async () => {
    const calls = server({ 'GET /issues/101/page': { body: issuePage(LOGIN) } });
    const screen = await render(BoardPage, { board: board() });
    const link = screen.getByRole('link', { name: 'WEB-101' });
    await expect.element(link).toBeVisible();
    link.element().addEventListener(
      'click',
      (event) => {
        event.preventDefault();
      },
      { once: true },
    );
    await userEvent.keyboard('{Control>}');
    await link.click();
    await userEvent.keyboard('{/Control}');
    expect(calls.some((one) => one.path === '/issues/101/page')).toBe(false);
  });
});

describe('BRD-080: the popup in edit mode', () => {
  it('F2 on a card opens the popup with the edit form', async () => {
    server({ 'GET /issues/101/page': { body: issuePage(LOGIN) } });
    const screen = await render(BoardPage, { board: board() });
    const link = screen.getByRole('link', { name: 'WEB-101' });
    await expect.element(link).toBeVisible();
    (link.element() as HTMLElement).focus();
    await userEvent.keyboard('{F2}');
    const popup = screen.getByRole('dialog', { name: 'WEB-101 Login form' });
    await expect.element(popup.getByRole('textbox', { name: 'Summary' })).toHaveValue('Login form');
  });
});

describe('BRD-077: creating in a cell', () => {
  it('«+» opens the cell form on its draft with the column locked; «Добавить» publishes', async () => {
    let published = false;
    const calls = server({
      'POST /boards/9/cell-draft': { status: 201, body: { issue: 900, created: true } },
      'GET /issues/900': { body: DRAFT },
      'PATCH /issues/900': (request) => ({
        body: { issue: { ...DRAFT, ...(request.body as object), text_version: 3 }, messages: [] },
      }),
      'POST /issues/900/publish': () => {
        published = true;
        return {
          body: {
            issue: { ...DRAFT, draft: false, readable_id: 'WEB-900', summary: 'Export' },
            messages: [],
          },
        };
      },
    });
    const screen = await render(BoardPage, { board: board() });
    await screen.getByRole('button', { name: 'Add a card to “Open”' }).click();
    const form = screen.getByRole('form', { name: 'New card' });
    const summary = form.getByRole('textbox', { name: 'Summary' });
    await expect.element(summary).toHaveFocus();
    expect(calls.find((one) => one.path === '/boards/9/cell-draft')?.body).toEqual({
      sprint: 7,
      column: 1,
    });
    await expect
      .element(form.getByRole('listitem'))
      .toHaveTextContent('Set by the cell: State: Open');
    await expectAccessible(form.element());

    await expect.element(form.getByRole('button', { name: 'Add', exact: true })).toBeDisabled();
    await summary.fill('Export');
    await userEvent.keyboard('{Control>}{Enter}{/Control}');
    await expect.element(form).not.toBeInTheDocument();
    expect(published).toBe(true);
    expect(
      calls.find((one) => one.method === 'PATCH' && one.path === '/issues/900')?.body,
    ).toMatchObject({ summary: 'Export' });
  });

  it('a refusal for required fields opens the fields and says so', async () => {
    server({
      'POST /boards/9/cell-draft': { body: { issue: 900, created: false } },
      'GET /issues/900': { body: DRAFT },
      'PATCH /issues/900': { body: { issue: { ...DRAFT, summary: 'Export' }, messages: [] } },
      'POST /issues/900/publish': failure(400, 'invalid_request', [
        { path: 'fields.11', problem: 'required', message: 'required' },
      ]),
    });
    const screen = await render(BoardPage, { board: board() });
    await screen.getByRole('button', { name: 'Add a card to “Open”' }).click();
    const form = screen.getByRole('form', { name: 'New card' });
    await form.getByRole('textbox', { name: 'Summary' }).fill('Export');
    await form.getByRole('button', { name: 'Add', exact: true }).click();
    await expect.element(form.getByText('Fill in the required fields')).toBeVisible();
    await expect
      .element(form.getByRole('button', { name: 'Fields' }))
      .toHaveAttribute('aria-expanded', 'true');
  });

  it('Esc closes an empty form and deletes its draft', async () => {
    const calls = server({
      'POST /boards/9/cell-draft': { status: 201, body: { issue: 900, created: true } },
      'GET /issues/900': { body: DRAFT },
      'DELETE /issues/900': { status: 204 },
    });
    const screen = await render(BoardPage, { board: board() });
    await screen.getByRole('button', { name: 'Add a card to “Open”' }).click();
    const form = screen.getByRole('form', { name: 'New card' });
    await expect.element(form.getByRole('textbox', { name: 'Summary' })).toHaveFocus();
    await userEvent.keyboard('{Escape}');
    await expect.element(form).not.toBeInTheDocument();
    await vi.waitFor(() => {
      expect(calls.some((one) => one.method === 'DELETE' && one.path === '/issues/900')).toBe(true);
    });
  });

  it('people who may not create issues see no «+»', async () => {
    server({}, view(), {
      global: [],
      projects: [{ project_id: 1, permissions: ['project.basic', 'issue.read'] }],
      capabilities: [],
    });
    const screen = await render(BoardPage, { board: board() });
    await expect.element(screen.getByRole('link', { name: 'WEB-101' })).toBeVisible();
    await expect
      .element(screen.getByRole('button', { name: 'Add a card to “Open”' }))
      .not.toBeInTheDocument();
  });
});

describe('BRD-076: the «Новая» menu', () => {
  it('offers a card, a sprint and a board; «Карточку» opens the form of the first column', async () => {
    server({
      'POST /boards/9/cell-draft': { status: 201, body: { issue: 900, created: true } },
      'GET /issues/900': { body: DRAFT },
    });
    const sprintBoard = board({ sprints_on: true });
    const screen = await render(BoardPage, { board: sprintBoard });
    await screen.getByRole('button', { name: 'New', exact: true }).click();
    const items = screen.getByRole('menuitem');
    await expect.element(items.first()).toBeVisible();
    expect(items.elements().map((one) => one.textContent.trim())).toEqual([
      'Card',
      'Sprint',
      'Board',
    ]);
    await screen.getByRole('menuitem', { name: 'Card' }).click();
    await expect.element(screen.getByRole('form', { name: 'New card' })).toBeVisible();
  });
});

describe('BRD-054: a new parent lane', () => {
  const parents: Board = board({ lanes: { ...NO_LANES, mode: 'parents' } });
  const laned = view({
    board: parents,
    lanes: [{ key: 'other', count: 4, estimate: null, issue: null }],
    lanes_total: 1,
    cards: view().cards.map((one) => ({ ...one, lane: 'other' })),
    cells: [{ column: 2, lane: 'other', total: 2, next_cursor: 'c2' }],
  });

  it('the footer button asks for a title and makes the lane', async () => {
    const calls = server(
      {
        'POST /boards/9/sprints/7/lanes': {
          status: 201,
          body: {
            lane: { key: '300', count: 0, estimate: null, issue: 300 },
            issue: issue(300, 'Checkout'),
            refs: { fields: [], values: [], users: [], groups: [] },
          },
        },
      },
      laned,
    );
    const screen = await render(BoardPage, { board: parents });
    await screen.getByRole('button', { name: 'New lane' }).click();
    const dialog = screen.getByRole('dialog', { name: 'New lane' });
    await dialog.getByRole('textbox', { name: 'Title' }).fill('Checkout');
    await dialog.getByRole('button', { name: 'Create' }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    expect(calls.find((one) => one.path === '/boards/9/sprints/7/lanes')?.body).toEqual({
      title: 'Checkout',
    });
  });
});

describe('BRD-023: the board health', () => {
  it('lists the findings with a link to the settings tab', async () => {
    server({});
    const sick = board({
      health: [
        { severity: 'warning', kind: 'archived_project', project: 1, value: null, slot: null },
        { severity: 'error', kind: 'field_missing', project: null, value: null, slot: 'lane' },
      ],
    });
    const screen = await render(BoardPage, { board: sick });
    const lane = screen.getByText('The field of lanes is in no project of the board.');
    await expect.element(lane).toBeVisible();
    await expect.element(screen.getByText('Project Web is archived.')).toBeVisible();
    await screen.getByRole('button', { name: 'Open' }).first().click();
    const settings = screen.getByRole('dialog', { name: 'Board settings' });
    await expect
      .element(settings.getByRole('tab', { name: 'Lanes' }))
      .toHaveAttribute('aria-selected', 'true');
  });

  it('readers see the findings without links', async () => {
    server({});
    const screen = await render(BoardPage, {
      board: board({
        rights: { delete: false, edit: false, share: false, transfer: false },
        health: [
          { severity: 'warning', kind: 'archived_project', project: 1, value: null, slot: null },
        ],
      }),
    });
    await expect.element(screen.getByText('Project Web is archived.')).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Open' })).not.toBeInTheDocument();
  });
});
