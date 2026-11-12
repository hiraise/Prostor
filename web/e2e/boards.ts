// Steps the board flows share: a project with issues, a kanban board on it with cards, and drags
// with a mouse or a finger.
import { expect, type Locator, type Page } from '@playwright/test';

import type { Board, BoardView, Issue, IssueAnswer, Project } from '../src/lib/api/schema.gen.ts';

export interface BoardSetup {
  project: Project;
  board: Board;
  sprint: number;
  issues: Issue[];
}

/** Runs `work` over `items`, `width` at a time. */
async function inBatches<T, R>(items: T[], width: number, work: (item: T) => Promise<R>) {
  const done: R[] = [];
  for (let start = 0; start < items.length; start += width) {
    done.push(...(await Promise.all(items.slice(start, start + width).map(work))));
  }
  return done;
}

/**
 * A blank project with `summaries` as issues and a kanban board on it with every issue a card:
 * `columnOf(index)` chooses the column by its position (the first one when absent). `extra`
 * columns with new values join the board first.
 */
export async function boardWithCards(
  page: Page,
  key: string,
  summaries: string[],
  options: { columnOf?: (index: number) => number; extra?: string[] } = {},
): Promise<BoardSetup> {
  const api = page.request;
  const made = await api.post('/api/v1/projects', {
    data: { name: `Board ${key}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const issues = await inBatches(summaries, 10, async (summary) => {
    const created = await api.post('/api/v1/issues', { data: { project: project.id, summary } });
    expect(created.status()).toBe(201);
    return ((await created.json()) as IssueAnswer).issue;
  });

  const createdBoard = await api.post('/api/v1/boards', {
    data: { preset: 'kanban', name: `Kanban ${key}`, projects: [project.id] },
  });
  expect(createdBoard.status()).toBe(201);
  const { id } = (await createdBoard.json()) as Board;
  for (const name of options.extra ?? []) {
    const column = await api.post(`/api/v1/boards/${String(id)}/columns`, {
      data: { new_value: name },
    });
    expect(column.status()).toBeLessThan(300);
  }
  const shown = await api.get(`/api/v1/boards/${String(id)}/view`);
  expect(shown.status()).toBe(200);
  const view = (await shown.json()) as BoardView;
  const sprint = view.sprint?.id;
  if (sprint === undefined) throw new Error('the board shows no sprint');
  const columns = view.board.columns;
  expect(columns.length).toBeGreaterThan(1);

  // Cards added at once may share a rank, so a small board adds them one by one, in order.
  await inBatches(
    issues.map((issue, index) => ({ issue, index })),
    issues.length <= 20 ? 1 : 10,
    async ({ issue, index }) => {
      const column = columns[options.columnOf?.(index) ?? 0];
      const added = await api.post(`/api/v1/boards/${String(id)}/sprints/${String(sprint)}/cards`, {
        data: { issue: issue.id, column: column?.id },
      });
      expect(added.status()).toBeLessThan(300);
    },
  );
  return { project, board: view.board, sprint, issues };
}

/** The cards of a column, by their summaries. */
export function cardsOf(page: Page, column: string): Locator {
  return page.getByRole('list', { name: `Cards of “${column}”` }).locator('article .summary');
}

/** The middle of an element's top edge, a few pixels in. */
async function topOf(locator: Locator): Promise<{ x: number; y: number }> {
  const box = await locator.boundingBox();
  if (!box) throw new Error('not on the screen');
  return { x: box.x + box.width / 2, y: box.y + 6 };
}

/** Drags a card with a mouse to the top of a target element (a cell or a card). */
export async function mouseDrag(page: Page, card: Locator, target: Locator): Promise<void> {
  await card.scrollIntoViewIfNeeded();
  const from = await topOf(card);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 10, from.y + 10, { steps: 3 });
  const to = await topOf(target);
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await page.mouse.up();
}

/** Drags a card with a finger: a long press, then the move (Chromium touch through CDP). */
export async function touchDrag(page: Page, card: Locator, target: Locator): Promise<void> {
  await card.scrollIntoViewIfNeeded();
  const cdp = await page.context().newCDPSession(page);
  const from = await topOf(card);
  const to = await topOf(target);
  const touch = (type: 'touchStart' | 'touchMove' | 'touchEnd', point?: { x: number; y: number }) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: point ? [{ x: point.x, y: point.y }] : [],
    });
  await touch('touchStart', from);
  await page.waitForTimeout(500);
  for (let step = 1; step <= 12; step += 1) {
    await touch('touchMove', {
      x: from.x + ((to.x - from.x) * step) / 12,
      y: from.y + ((to.y - from.y) * step) / 12,
    });
    await page.waitForTimeout(16);
  }
  await touch('touchEnd');
  await cdp.detach();
}
