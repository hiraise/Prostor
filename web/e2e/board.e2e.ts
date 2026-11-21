// The board end to end (step 1.8a): the cards of a kanban board, a drag to the next column that
// changes the issue's state, a reorder within a cell, both seen live in a second browser, a card
// taken off the board; on a phone the one-column zoom with its strip, the overview and a long-press
// drag. Runs on a desktop and on a phone.
import { expect, test } from '@playwright/test';

import type { Issue } from '../src/lib/api/schema.gen.ts';
import { boardWithCards, cardsOf, mouseDrag, touchDrag } from './boards.ts';
import { signIn } from './issues.ts';

test('BRD-001, BRD-068, BRD-069, BRD-074, BRD-088, BRD-114: a board', async ({
  page,
  browser,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const key = `B${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  await signIn(page);
  const { board, sprint, issues } = await boardWithCards(page, key, ['Alpha', 'Beta', 'Gamma']);
  const [first, second] = board.columns.map((column) => column.value.name);
  if (!first || !second) throw new Error('the board has fewer than two columns');
  const beta = issues[1];
  if (!beta) throw new Error('no issue');

  // A second person's browser follows the same board.
  const other = await browser.newContext();
  const watcher = await other.newPage();
  await signIn(watcher);
  await watcher.goto(`/boards/${String(board.id)}`);
  await expect(cardsOf(watcher, first)).toHaveText(['Alpha', 'Beta', 'Gamma']);

  await page.goto(`/boards/${String(board.id)}`);
  await expect(page.getByRole('heading', { level: 1 })).toContainText(board.name);
  await expect(cardsOf(page, first)).toHaveText(['Alpha', 'Beta', 'Gamma']);
  const alphaCard = page.locator('[data-card]').filter({ hasText: 'Alpha' });
  await expect(alphaCard.getByText('No estimate', { exact: true })).toBeVisible();
  await expect(alphaCard.getByRole('button', { name: /Actions of/ })).toHaveCount(0);

  const grid = page.getByRole('region', { name: 'Columns' });
  if (phone) {
    // One column at a time; the strip names them and follows a swipe.
    const strip = page.getByRole('navigation', { name: 'Board columns' });
    await expect(strip.getByRole('button', { name: new RegExp(`^${first}`) })).toHaveAttribute(
      'aria-current',
      'true',
    );
    await grid.evaluate((element) => {
      element.scrollBy({ left: element.clientWidth });
    });
    await expect(strip.getByRole('button', { name: new RegExp(`^${second}`) })).toHaveAttribute(
      'aria-current',
      'true',
    );
    // The overview shows every column narrow; a long press drags a card across.
    await page.getByRole('button', { name: 'All', exact: true }).click();
    await expect(page.getByRole('button', { name: 'All', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await touchDrag(
      page,
      cardsOf(page, first).filter({ hasText: 'Beta' }),
      page.locator(`section[aria-label="${second}"] [data-drop-column]`),
    );
  } else {
    await mouseDrag(
      page,
      cardsOf(page, first).filter({ hasText: 'Beta' }),
      page.locator(`section[aria-label="${second}"] [data-drop-column]`),
    );
  }

  // The card moved here and there, and the issue's state is the column's value.
  await expect(cardsOf(page, second)).toHaveText(['Beta']);
  await expect(cardsOf(watcher, second)).toHaveText(['Beta']);
  await expect(cardsOf(watcher, first)).toHaveText(['Alpha', 'Gamma']);
  await expect
    .poll(async () => {
      const answer = await page.request.get(`/api/v1/issues/${String(beta.id)}`);
      return JSON.stringify((await answer.json()) as Issue).includes(`"${second}"`);
    })
    .toBe(true);

  // Gamma goes above Alpha within the cell.
  const drag = phone ? touchDrag : mouseDrag;
  await drag(
    page,
    cardsOf(page, first).filter({ hasText: 'Gamma' }),
    // The top of Alpha's card, above its middle.
    page.locator('[data-card]').filter({ hasText: 'Alpha' }),
  );
  await expect(cardsOf(page, first)).toHaveText(['Gamma', 'Alpha']);
  await expect(cardsOf(watcher, first)).toHaveText(['Gamma', 'Alpha']);

  // Alpha leaves through the API; both open boards receive the change.
  const alpha = issues[0];
  if (!alpha) throw new Error('no issue');
  const removed = await page.request.delete(
    `/api/v1/boards/${String(board.id)}/sprints/${String(sprint)}/cards/${String(alpha.id)}`,
  );
  expect(removed.status()).toBe(204);
  await expect(cardsOf(page, first)).toHaveText(['Gamma']);
  await expect(cardsOf(watcher, first)).toHaveText(['Gamma']);

  await other.close();
});
