// The board screen on a desktop and on a phone (step 1.8a-5; BRD-046, BRD-047, BRD-057, BRD-058,
// BRD-068, BRD-082, BRD-085).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { BoardView } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { need } from '../../../../test/dom.ts';
import { fakeApi, failure, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { columnQuery, plainExcerpt } from '../boards.ts';
import { COLUMNS, MEMBER, OPEN, SESSION, board, column, issue, view } from '../test-data.ts';
import BoardPage from './BoardPage.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn(() => Promise.resolve()) }));

function server(routes: Record<string, FakeRoute>, shown: BoardView = view()): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi({ 'GET /boards/9/view': { body: shown }, ...routes });
}

function cardsOf(screen: Awaited<ReturnType<typeof render>>, name: string): string[] {
  return screen
    .getByRole('list', { name: `Cards of “${name}”` })
    .getByRole('article')
    .elements()
    .map((card) => card.querySelector('.summary')?.textContent ?? '');
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('BRD-068: the board', () => {
  it('shows the columns with their cards in the shared order', async () => {
    server({});
    const screen = await render(BoardPage, { board: board() });

    await expect.element(screen.getByRole('region', { name: 'Columns' })).toBeVisible();
    for (const name of ['Open', 'In Progress', 'Done']) {
      await expect.element(screen.getByRole('heading', { level: 2, name })).toBeVisible();
    }
    expect(cardsOf(screen, 'Open')).toEqual(['Login form', 'Logout']);
    expect(cardsOf(screen, 'Done')).toEqual(['Done already']);
    // The ID links to the issue page; a resolved one is struck through and said in words.
    const link = screen.getByRole('link', { name: 'WEB-101' });
    expect(link.element().getAttribute('href')).toBe('/issues/WEB-101');
    await expect.element(screen.getByRole('link', { name: 'WEB-104 (resolved)' })).toBeVisible();
    const card = screen.getByRole('article', { name: 'WEB-101' });
    const summary = card.getByText('Login form', { exact: true }).element();
    expect(summary.getBoundingClientRect().top).toBeCloseTo(
      link.element().getBoundingClientRect().top,
      0,
    );
    expect(card.getByRole('button', { name: 'Actions of WEB-101' }).elements()).toHaveLength(0);
    // The ID link covers its whole card, so its own text box may be small.
    const small = await smallTargets(screen.container, isPhone() ? 44 : 24);
    expect(small.filter((name) => !name.startsWith('WEB-'))).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('shows the board name in the switcher in full, and cuts only a truly long one', async () => {
    server({});
    const screen = await render(BoardPage, { board: board({ name: 'Prostor' }) });
    const switcherTitle = () =>
      need(screen.container.querySelector('h1 button span'), 'switcher title');
    // A free row must not truncate the name (the switcher used to collapse to “Trackfl…”).
    expect(switcherTitle().scrollWidth).toBeLessThanOrEqual(switcherTitle().clientWidth);

    const long = 'A very long board name that cannot fit the header row of the board page. '.repeat(
      3,
    );
    await screen.rerender({ board: board({ name: long }) });
    // A definite narrow width, so the long name truly cannot fit the row.
    screen.container.style.width = '640px';
    expect(switcherTitle().scrollWidth).toBeGreaterThan(switcherTitle().clientWidth);
  });

  it('asks the person to choose columns when the board has none', async () => {
    const empty = board({ column_field: null, columns: [] });
    server({}, view({ board: empty, columns: [], cards: [], cells: [], issues: [] }));
    const screen = await render(BoardPage, { board: empty });
    await expect.element(screen.getByText('The board has no columns yet')).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Open settings' })).toBeVisible();
  });

  it('says when the board does not load, and tries again', async () => {
    let fail = true;
    server({
      'GET /boards/9/view': () => (fail ? failure(500, 'internal') : { body: view() }),
    });
    const screen = await render(BoardPage, { board: board() });
    await expect.element(screen.getByText('Could not load the board.')).toBeVisible();
    fail = false;
    await screen.getByRole('button', { name: 'Try again' }).click();
    await expect.element(screen.getByRole('region', { name: 'Columns' })).toBeVisible();
  });
});

describe('BRD-057: a truncated cell', () => {
  it('shows «N more» and the next page from the view’s cursor', async () => {
    const calls = server({
      'GET /boards/9/cell': {
        body: {
          cards: [{ issue: 105, column: 2, lane: null, rank: 'b' }],
          issues: [issue(105, 'Filters')],
          refs: { fields: [], values: [], users: [], groups: [] },
          next_cursor: null,
        },
      },
    });
    const screen = await render(BoardPage, { board: board() });

    await screen.getByRole('button', { name: '1 more' }).click();
    await expect.poll(() => cardsOf(screen, 'In Progress')).toEqual(['Search', 'Filters']);
    const asked = calls.find((call) => call.path === '/boards/9/cell');
    expect(asked?.query.get('column')).toBe('2');
    expect(asked?.query.get('cursor')).toBe('c2');
    await expect.element(screen.getByRole('button', { name: '1 more' })).not.toBeInTheDocument();
  });
});

describe('BRD-046: limits', () => {
  it('names the limit and warns in words when a column breaks it', async () => {
    server({});
    const screen = await render(BoardPage, { board: board() });
    const doing = screen.getByRole('region', { name: 'In Progress' });
    await expect.element(doing.getByText('Over the limit of 1')).toBeVisible();
    await expect.element(doing.getByText('2 cards')).toBeInTheDocument();
    // «Открыть списком»: the board projects and the column value.
    const list = doing.getByRole('link', { name: 'Open “In Progress” as a list' });
    expect(decodeURIComponent(list.element().getAttribute('href') ?? '')).toBe(
      '/issues?q=project: WEB State: "In Progress"'.replaceAll(' ', '+'),
    );
  });
});

describe('BRD-047: collapsing a column', () => {
  // A phone shows one column at a time or the overview instead (BRD-088, phone.browser.test.ts).
  it.skipIf(isPhone())(
    'collapses for this person and saves it; a failed save puts it back',
    async () => {
      let refuse = false;
      const calls = server({
        'PATCH /boards/9/profile': (request) =>
          refuse
            ? failure(500, 'internal')
            : {
                body: {
                  ...view().profile,
                  ...(request.body as object),
                },
              },
      });
      const screen = await render(BoardPage, { board: board() });

      await screen.getByRole('button', { name: 'Collapse “Open”' }).click();
      await expect
        .element(screen.getByRole('button', { name: 'Expand “Open”' }))
        .toHaveAttribute('aria-expanded', 'false');
      expect(screen.getByRole('list', { name: 'Cards of “Open”' }).elements()).toHaveLength(0);
      expect(calls.find((call) => call.method === 'PATCH')?.body).toEqual({
        collapsed_columns: [1],
      });
      await expectAccessible(screen.container);

      refuse = true;
      await screen.getByRole('button', { name: 'Expand “Open”' }).click();
      await expect.element(screen.getByRole('button', { name: 'Expand “Open”' })).toBeVisible();
    },
  );
});

describe('BRD-058: cards', () => {
  it('grow with the size: fields from medium, tags from large, the description at the largest', async () => {
    const calls = server({
      'PATCH /boards/9/profile': (request) => ({
        body: { ...view().profile, ...(request.body as object) },
      }),
    });
    const screen = await render(BoardPage, { board: board() });
    const card = screen.getByRole('article', { name: 'WEB-101' });

    // Medium: the assignee, the estimation and the priority; no tags yet.
    await expect.element(card.getByRole('img', { name: 'Assignee: Boris Lead' })).toBeVisible();
    await expect.element(card.getByText('1h 30m')).toBeVisible();
    await expect.element(card.getByText('Major')).toBeVisible();
    expect(card.getByRole('list', { name: 'Tags' }).elements()).toHaveLength(0);

    await screen.getByRole('button', { name: /Card size/ }).click();
    await screen.getByRole('menuitemradio', { name: 'Small' }).click();
    await expect.element(card.getByText('Major')).not.toBeInTheDocument();
    await expect.element(card.getByText('1h 30m')).toBeVisible();

    await screen.getByRole('button', { name: /Card size/ }).click();
    await screen.getByRole('menuitemradio', { name: 'Extra large' }).click();
    await expect.element(card.getByRole('list', { name: 'Tags' })).toBeVisible();
    await expect.element(card.getByText(/People cannot sign in\./)).toBeVisible();
    expect(calls.filter((call) => call.method === 'PATCH').map((call) => call.body)).toEqual([
      { card_size: 0 },
      { card_size: 3 },
    ]);
    await expectAccessible(screen.container);
  });

  it('a description excerpt drops the marks of Markdown', () => {
    expect(
      plainExcerpt('## Why\nPeople **cannot** sign in.\n\nSee [the log](http://x).\nMore'),
    ).toBe('Why\nPeople cannot sign in.\nSee the log.');
    expect(columnQuery(board({ column_field: null }), COLUMNS[0] ?? column(1, OPEN))).toBe(
      'project: WEB',
    );
  });
});

describe('BRD-085: progress', () => {
  it('shows one part per column, the broken one marked, each with its count', async () => {
    server({});
    const screen = await render(BoardPage, { board: board() });
    const bar = screen.getByRole('list', { name: 'Progress by column' });
    await expect.element(bar).toBeVisible();
    expect(
      bar
        .getByRole('listitem')
        .elements()
        .map((part) => part.textContent.trim()),
    ).toEqual(['Open: 2 cards', 'In Progress: 2 cards', 'Done: 1 card']);
    expect(bar.getByRole('listitem').nth(1).element().classList.contains('broken')).toBe(true);
  });
});

describe('BRD-082: the toolbar', () => {
  it('without edit rights the gear says why and opens nothing', async () => {
    const reader = board({ rights: { delete: false, edit: false, share: false, transfer: false } });
    server({}, view({ board: reader }));
    const screen = await render(BoardPage, { board: reader });
    const gear = screen.getByRole('button', {
      name: 'Only those who change its settings can open them',
    });
    await expect.element(gear).toHaveAttribute('aria-disabled', 'true');
    await gear.click({ force: true });
    expect(screen.getByRole('dialog').elements()).toHaveLength(0);
    await expectAccessible(screen.container);
  });
});
