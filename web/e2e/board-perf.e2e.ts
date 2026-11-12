// The frame check of the board (step 1.8a, architecture §7: a board of 500 cards scrolls and drags
// at 60 fps). A board of 500 cards in five columns, built through the API; the page watches long
// animation frames (long tasks where the browser has no such entries) while it scrolls and while a
// card is dragged across the columns: no frame of the drag may take over 50 ms. Desktop only: the
// phone shows one column at a time.
import { expect, test, type Page } from '@playwright/test';

import { boardWithCards, cardsOf } from './boards.ts';
import { signIn } from './issues.ts';

const CARDS = 500;
const COLUMNS = 5;
const LONG_FRAME_MS = 50;

declare global {
  interface Window {
    longFrames?: number[];
  }
}

/** Starts collecting the durations of long frames from now on. */
async function watchFrames(page: Page): Promise<void> {
  await page.evaluate(() => {
    window.longFrames = [];
    const kind = PerformanceObserver.supportedEntryTypes.includes('long-animation-frame')
      ? 'long-animation-frame'
      : 'longtask';
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.longFrames?.push(entry.duration);
    }).observe({ type: kind });
  });
}

/** The durations collected since the last call, and starts again. */
async function takeFrames(page: Page): Promise<number[]> {
  return page.evaluate(() => {
    const taken = window.longFrames ?? [];
    window.longFrames = [];
    return taken;
  });
}

test('BRD-068: a board of 500 cards scrolls and drags without long frames', async ({
  page,
}, testInfo) => {
  test.skip(testInfo.project.name === 'phone', 'the phone shows one column at a time');
  test.setTimeout(180_000);
  const key = `F${String(Date.now()).slice(-6)}`;
  await signIn(page);
  const summaries = Array.from({ length: CARDS }, (_, index) => `Card ${String(index + 1)}`);
  const { board } = await boardWithCards(page, key, summaries, {
    columnOf: (index) => index % COLUMNS,
    extra: ['Review'],
  });
  const names = board.columns.map((column) => column.value.name);
  expect(names.length).toBeGreaterThanOrEqual(COLUMNS);

  await page.goto(`/boards/${String(board.id)}`);
  for (const name of names.slice(0, COLUMNS)) {
    await expect(cardsOf(page, name)).toHaveCount(CARDS / COLUMNS);
  }
  await page.waitForTimeout(500);
  await watchFrames(page);
  await takeFrames(page);

  // Scrolling down the board and across its columns.
  const grid = page.getByRole('region', { name: 'Columns' });
  const box = await grid.boundingBox();
  if (!box) throw new Error('no grid');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  for (let step = 0; step < 20; step += 1) {
    await page.mouse.wheel(0, 400);
    await page.waitForTimeout(16);
  }
  for (let step = 0; step < 10; step += 1) {
    await page.mouse.wheel(300, 0);
    await page.waitForTimeout(16);
  }
  await page.waitForTimeout(300);
  const scrolling = await takeFrames(page);
  await grid.evaluate((element) => {
    element.scrollTo(0, 0);
  });
  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });
  await page.waitForTimeout(300);
  await takeFrames(page);

  // Dragging the first card of the first column over every column and down the last one.
  const card = cardsOf(page, names[0] ?? '').first();
  const from = await card.boundingBox();
  if (!from) throw new Error('no card');
  const start = { x: from.x + 20, y: from.y + 8 };
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(start.x + 10, start.y + 10, { steps: 2 });
  const end = { x: box.x + box.width - 80, y: box.y + box.height - 80 };
  await page.mouse.move(end.x, end.y, { steps: 90 });
  await page.waitForTimeout(500);
  const dragging = await takeFrames(page);
  await page.keyboard.press('Escape');
  await page.mouse.up();

  testInfo.annotations.push({
    type: 'frames',
    description: `long frames: scroll ${JSON.stringify(scrolling.map(Math.round))}, drag ${JSON.stringify(dragging.map(Math.round))}`,
  });
  expect(Math.max(0, ...dragging)).toBeLessThanOrEqual(LONG_FRAME_MS);
});
