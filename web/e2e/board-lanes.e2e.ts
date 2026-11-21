// Lanes and creating on a board end to end (step 1.8c): a scrum board with parent lanes; a card
// moves to another lane and becomes that issue's subtask (a drag on a desktop, an API move on a
// phone); a card is created in a cell of a lane; it opens in the issue popup and is edited
// there. Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import type {
  Board,
  BoardList,
  BoardView,
  Issue,
  IssueAnswer,
  PlacedLane,
  Project,
} from '../src/lib/api/schema.gen.ts';
import { mouseDrag } from './boards.ts';
import { signIn } from './issues.ts';

async function json<T>(
  answer: Promise<{ status(): number; json(): Promise<unknown> }>,
  status = 200,
) {
  const done = await answer;
  expect(done.status()).toBe(status);
  return (await done.json()) as T;
}

/** The cards of a cell, by their summaries. */
function cellCards(page: Page, column: string, lane: string) {
  return page
    .getByRole('list', { name: `Cards of “${column}” in lane “${lane}”` })
    .locator('article .summary');
}

/** The parent of an issue, by its summary. */
async function parentOf(page: Page, issue: number): Promise<string | null> {
  const found = await json<Issue>(page.request.get(`/api/v1/issues/${String(issue)}`));
  return found.parent?.summary ?? null;
}

test('BRD-050, BRD-054, BRD-077, BRD-079, BRD-080, BRD-096: parent lanes and the popup', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const key = `L${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  await signIn(page);
  const api = page.request;

  // A scrum project with its board, turned to parent lanes, with two lanes and a card in one.
  const project = await json<Project>(
    api.post('/api/v1/projects', { data: { name: `Lanes ${key}`, key, template: 'scrum' } }),
    201,
  );
  const alpha = (
    await json<IssueAnswer>(
      api.post('/api/v1/issues', { data: { project: project.id, summary: 'Alpha' } }),
      201,
    )
  ).issue;
  const list = await json<BoardList>(api.get(`/api/v1/boards?project=${String(project.id)}`));
  const found = list.items[0];
  if (!found) throw new Error('the template made no board');
  const id = String(found.id);
  const board = await json<Board>(api.get(`/api/v1/boards/${id}`));
  await json<Board>(
    api.patch(`/api/v1/boards/${id}`, {
      data: { lanes: { mode: 'parents', other: 'top' }, version: board.version },
    }),
  );
  const view = await json<BoardView>(api.get(`/api/v1/boards/${id}/view`));
  const sprint = view.sprint?.id;
  if (sprint === undefined) throw new Error('the scrum board has no sprint');
  const column = view.board.columns[0]?.value.name ?? '';
  const lanes = `/api/v1/boards/${id}/sprints/${String(sprint)}/lanes`;
  const login = await json<PlacedLane>(api.post(lanes, { data: { title: 'Login' } }), 201);
  const checkout = await json<PlacedLane>(api.post(lanes, { data: { title: 'Checkout' } }), 201);
  const columnId = String(view.board.columns[0]?.id ?? 0);
  await json<unknown>(
    api.post(`/api/v1/boards/${id}/sprints/${String(sprint)}/cards`, {
      data: { issue: alpha.id, lane: login.lane.key },
    }),
  );

  await page.goto(`/boards/${id}`);
  await expect(cellCards(page, column, 'Login')).toHaveText(['Alpha']);

  // Alpha moves to «Checkout» and becomes its subtask.
  if (phone) {
    const current = await json<Issue>(api.get(`/api/v1/issues/${String(alpha.id)}`));
    const moved = await api.post(
      `/api/v1/boards/${id}/sprints/${String(sprint)}/cards/${String(alpha.id)}/move`,
      {
        data: {
          column: Number(columnId),
          lane: checkout.lane.key,
          index: 0,
          version: current.version,
        },
      },
    );
    expect(moved.status()).toBeLessThan(300);
  } else {
    await mouseDrag(
      page,
      cellCards(page, column, 'Login').filter({ hasText: 'Alpha' }),
      // An empty cell of a lane has no list: the cell itself takes the drop.
      page.locator(`[data-drop-column="${columnId}"][data-drop-lane="${checkout.lane.key}"]`),
    );
  }
  await expect(cellCards(page, column, 'Checkout')).toHaveText(['Alpha']);
  await expect.poll(() => parentOf(page, alpha.id)).toBe('Checkout');

  // A card made in the cell of «Login» is a subtask of it: the new-issue form opens on the
  // cell's draft, and «Create» publishes and opens the new issue's page. Back on the board
  // the card sits in its cell.
  await page.getByRole('button', { name: `Add a card to “${column} · Login”` }).click();
  const form = page.getByRole('form', { name: 'New issue form' });
  await form.getByRole('textbox', { name: 'Summary' }).fill('Delta');
  await form.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(form).toBeHidden();
  await expect(page.getByRole('heading', { level: 1, name: 'Delta' })).toBeVisible();
  await page.goto(`/boards/${id}`);
  await expect(cellCards(page, column, 'Login')).toHaveText(['Delta']);
  const delta = page.locator('article').filter({ hasText: 'Delta' });
  const deltaId = (await delta.locator('.id a').textContent())?.trim() ?? '';
  const made = await json<Issue>(api.get(`/api/v1/issues/${deltaId}`));
  expect(made.parent?.summary).toBe('Login');

  // The card opens in the popup; its edit form changes the summary.
  await delta.locator('.id a').click();
  const popup = page.getByRole('dialog', { name: new RegExp(`^${deltaId} Delta`) });
  await expect(popup.getByRole('heading', { name: 'Delta', level: 1 })).toBeVisible();
  await popup.getByRole('button', { name: 'Edit', exact: true }).click();
  await popup.getByRole('textbox', { name: 'Summary' }).fill('Delta two');
  await popup.getByRole('button', { name: 'Save' }).click();
  await expect(popup.getByRole('heading', { name: 'Delta two', level: 1 })).toBeVisible();
  await popup.getByRole('button', { name: 'Close' }).click();
  await expect(popup).toBeHidden();
  await expect(cellCards(page, column, 'Login')).toHaveText(['Delta two']);
  expect(page.url()).toContain(`/boards/${id}`);
});
