// Creating on the board, the issue popup and the board's health (step 1.8c-5; BRD-023, BRD-054,
// BRD-076, BRD-077, BRD-079, BRD-080), on a desktop and on a phone.
import { userEvent } from 'vitest/browser';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import { goto } from '$app/navigation';

import type { AccessMap, Board, BoardView, QueryProblem } from '#lib/api/schema.gen.ts';
import { newIssueDialog } from '#lib/features/issues/new/open.svelte.ts';
import { issue as fullIssue, issuePage } from '#lib/features/issues/test-data.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone } from '../../../../test/a11y.ts';
import { fakeApi, failure, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import BoardPage from '../board/BoardPage.svelte';
import { headerFilter } from '../board/header-filter.svelte.ts';
import { NO_LANES, SESSION, board, issue, view } from '../test-data.ts';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

/** A create that opens the new-issue form: the dialog on wide screens, its page on phones. */
async function expectNewIssueOn(draft: number): Promise<void> {
  if (isPhone()) {
    await vi.waitFor(() => {
      expect(vi.mocked(goto)).toHaveBeenCalledWith(`/issues/new?draft=${String(draft)}`);
    });
    return;
  }
  await vi.waitFor(() => {
    expect(newIssueDialog.open).toBe(true);
  });
  expect(newIssueDialog.target).toEqual({ draft });
  newIssueDialog.open = false;
}

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

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  toasts.clear();
  headerFilter.clear();
  newIssueDialog.open = false;
  vi.mocked(goto).mockClear();
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
    // «Открыть страницей» opens a new tab instead of leaving the board.
    const openPage = popup.getByRole('link', { name: 'Open the page' });
    await expect.element(openPage).toHaveAttribute('href', '/issues/WEB-101');
    await expect.element(openPage).toHaveAttribute('target', '_blank');
    await expect.element(openPage).toHaveAttribute('rel', 'noopener');
    // No breadcrumbs inside the board.
    await expect
      .element(popup.getByRole('navigation', { name: 'Breadcrumbs' }))
      .not.toBeInTheDocument();
    if (!isPhone()) {
      const facts = popup.element().querySelectorAll('.facts > div');
      const created = facts[0];
      const updated = facts[1];
      if (!created || !updated) throw new Error('the popup has no issue metadata');
      expect(updated.getBoundingClientRect().top).toBeCloseTo(
        created.getBoundingClientRect().top,
        0,
      );
      const panel = popup.getByRole('complementary', { name: 'Fields' }).element();
      const panelFrame = panel.parentElement;
      const body = panel.closest('.body');
      if (!panelFrame || !body) throw new Error('the popup has no field panel');
      const bottom = body.getBoundingClientRect().bottom;
      expect(panelFrame.getBoundingClientRect().bottom).toBeCloseTo(
        bottom - parseFloat(getComputedStyle(body).paddingBottom),
        0,
      );
    }
    await expectAccessible(popup.element());
    await userEvent.keyboard('{Escape}');
    await expect.element(popup).not.toBeInTheDocument();
  });

  it('«Ещё» → «Команды…» opens the command dialog over the popup (2026-11-15)', async () => {
    server({
      'GET /issues/101/page': { body: issuePage(LOGIN) },
      'POST /commands/assist': { body: { operations: [], problems: [], suggestions: [] } },
    });
    const screen = await render(BoardPage, { board: board() });
    await screen.getByRole('link', { name: 'WEB-101' }).click();
    const popup = screen.getByRole('dialog', { name: 'WEB-101 Login form' });
    await popup.getByRole('button', { name: 'More' }).click();
    await screen.getByRole('menuitem', { name: 'Commands…' }).click();
    const commands = screen.getByRole('dialog', { name: 'Commands' });
    await expect.element(commands).toBeVisible();
    await expect.element(commands.getByRole('combobox', { name: 'Command' })).toBeVisible();
  });

  it('the `issue` param of the address reopens the card after a reload (2026-11-15)', async () => {
    server({ 'GET /issues/101/page': { body: issuePage(LOGIN) } });
    const screen = await render(BoardPage, { board: board(), card: '101' });
    await expect.element(screen.getByRole('dialog', { name: 'WEB-101 Login form' })).toBeVisible();
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
  it('F2 opens the edit form and Save publishes the changed summary', async () => {
    server({
      'GET /issues/101/page': { body: issuePage(LOGIN) },
      'PATCH /issues/101': ({ body }) => ({
        body: {
          issue: { ...LOGIN, ...(body as object), text_version: LOGIN.text_version + 1 },
          messages: [],
        },
      }),
    });
    const screen = await render(BoardPage, { board: board() });
    const link = screen.getByRole('link', { name: 'WEB-101' });
    await expect.element(link).toBeVisible();
    (link.element() as HTMLElement).focus();
    await userEvent.keyboard('{F2}');
    const popup = screen.getByRole('dialog', { name: 'WEB-101 Login form' });
    const editSummary = popup.getByRole('textbox', { name: 'Summary' });
    await expect.element(editSummary).toHaveValue('Login form');
    const summaryStyle = getComputedStyle(editSummary.element());
    expect(summaryStyle.height).toBe('48px');
    expect(summaryStyle.outlineStyle).toBe('none');
    expect(summaryStyle.borderBottomWidth).toBe('1px');
    expect(editSummary.element().closest('.title')).not.toBeNull();
    expect(editSummary.element().closest('.content')).toBeNull();
    expect(popup.element().querySelectorAll('.title h1')).toHaveLength(1);
    if (!isPhone()) {
      const main = popup.element().querySelector('.main');
      if (!main) throw new Error('no popup main column');
      const style = getComputedStyle(main);
      expect(style.scrollbarGutter).toBe('stable');
      expect(parseFloat(style.paddingInlineEnd)).toBeGreaterThan(0);
    }
    await popup.getByRole('textbox', { name: 'Summary' }).fill('Login form two');
    await popup.getByRole('button', { name: 'Save' }).click();
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Login form two' }))
      .toBeVisible();
  });
});

describe('BRD-077: creating in a cell', () => {
  it('«+» starts the cell draft and opens the new-issue form on it', async () => {
    const calls = server({
      'POST /boards/9/cell-draft': { status: 201, body: { issue: 900, created: true } },
    });
    const screen = await render(BoardPage, { board: board() });
    await screen.getByRole('button', { name: 'Add a card to “Open”' }).click();
    expect(calls.find((one) => one.path === '/boards/9/cell-draft')?.body).toEqual({
      sprint: 7,
      column: 1,
    });
    await expectNewIssueOn(900);
  });

  it('a refused cell draft says so and opens nothing', async () => {
    server({ 'POST /boards/9/cell-draft': failure(403, 'forbidden', []) });
    const screen = await render(BoardPage, { board: board() });
    await screen.getByRole('button', { name: 'Add a card to “Open”' }).click();
    await vi.waitFor(() => {
      expect(toasts.items.at(-1)?.message).toBe('Could not start a card here');
    });
    expect(newIssueDialog.open).toBe(false);
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

describe('BRD-076: the «Новая» button', () => {
  it('opens the new-issue form on the first column', async () => {
    const calls = server({
      'POST /boards/9/cell-draft': { status: 201, body: { issue: 900, created: true } },
    });
    const screen = await render(BoardPage, { board: board() });
    await screen.getByRole('button', { name: 'New', exact: true }).click();
    expect(calls.find((one) => one.path === '/boards/9/cell-draft')?.body).toEqual({
      sprint: 7,
      column: 1,
    });
    await expectNewIssueOn(900);
  });
});

describe('BRD-081: the header filter names its mistakes', () => {
  it('a refused query lists its problems; the cards stay and no server error shows', async () => {
    const problems: QueryProblem[] = [
      { from: 0, to: 4, code: 'unknown_attribute', severity: 'error' },
    ];
    server({
      'GET /boards/9/view': (request) =>
        request.query.get('q')
          ? { status: 400, body: { code: 'invalid_query', message: 'x', problems } }
          : { body: view() },
    });
    const screen = await render(BoardPage, { board: board() });
    await expect.element(screen.getByRole('link', { name: 'WEB-101' })).toBeVisible();
    headerFilter.set('name: X');
    await expect.element(screen.getByText('“name”: no such field')).toBeVisible();
    await expect.element(screen.getByText(/something went wrong/i)).not.toBeInTheDocument();
    // The view before the filter stays on screen.
    await expect.element(screen.getByRole('link', { name: 'WEB-101' })).toBeVisible();
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
