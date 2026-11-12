// Sprints and the backlog end to end (step 1.8b): a scrum project comes with its board and backlog;
// two issues are planned into the sprint from the backlog (a drag on a desktop, the row's button
// on a phone); the sprint is finished and its unresolved issue carried to the next one; an issue
// joins a sprint from its own page; a query board previews the field changes before an add.
// Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import type {
  Board,
  BoardList,
  BoardView,
  Issue,
  IssueAnswer,
  Project,
  Sprint,
} from '../src/lib/api/schema.gen.ts';
import { cardsOf, mouseDrag } from './boards.ts';
import { signIn } from './issues.ts';

async function json<T>(
  answer: Promise<{ status(): number; json(): Promise<unknown> }>,
  status = 200,
) {
  const done = await answer;
  expect(done.status()).toBe(status);
  return (await done.json()) as T;
}

/** A project from `template` with `summaries` as issues, and the board it came with. */
async function templateProject(
  page: Page,
  key: string,
  template: 'scrum' | 'kanban',
  summaries: string[],
): Promise<{ project: Project; board: Board; issues: Issue[] }> {
  const api = page.request;
  const project = await json<Project>(
    api.post('/api/v1/projects', { data: { name: `Sprints ${key}`, key, template } }),
    201,
  );
  const issues: Issue[] = [];
  for (const summary of summaries) {
    issues.push(
      (
        await json<IssueAnswer>(
          api.post('/api/v1/issues', { data: { project: project.id, summary } }),
          201,
        )
      ).issue,
    );
  }
  const list = await json<BoardList>(api.get(`/api/v1/boards?project=${String(project.id)}`));
  const found = list.items[0];
  if (!found) throw new Error('the template made no board');
  const board = await json<Board>(api.get(`/api/v1/boards/${String(found.id)}`));
  expect(board.backlog).not.toBeNull();
  return { project, board, issues };
}

/** Opens the backlog panel (a sheet on phones) and gives its rows. */
async function openBacklog(page: Page) {
  await page.getByRole('button', { name: 'Backlog', exact: true }).click();
  return page.getByRole('list', { name: 'Backlog issues' });
}

test('BRD-089, BRD-098, BRD-100, BRD-101, ISS-020: sprints and the backlog', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const key = `S${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  await signIn(page);
  const api = page.request;
  const { board, issues } = await templateProject(page, key, 'scrum', ['Alpha', 'Beta', 'Gamma']);
  const [alpha, beta, gamma] = issues;
  if (!alpha || !beta || !gamma) throw new Error('no issues');
  const id = String(board.id);
  const first = await json<BoardView>(api.get(`/api/v1/boards/${id}/view`));
  const sprint = first.sprint;
  if (!sprint) throw new Error('the scrum board has no sprint');
  const next = await json<Sprint>(
    api.post(`/api/v1/boards/${id}/sprints`, { data: { name: 'Sprint next' } }),
    201,
  );
  const [todo, ...rest] = board.columns.map((column) => column.value.name);
  const done = board.columns.at(-1);
  if (!todo || rest.length === 0 || !done) throw new Error('the board has fewer than two columns');

  // Plan Alpha and Beta into the sprint from the backlog.
  await page.goto(`/boards/${id}`);
  await expect(page.getByRole('heading', { level: 1 })).toContainText(board.name);
  const rows = await openBacklog(page);
  await expect(rows.getByRole('listitem')).toHaveCount(3);
  for (const issue of [alpha, beta]) {
    if (phone) {
      await page
        .getByRole('button', { name: `Add ${issue.readable_id ?? ''} to the sprint` })
        .click();
    } else {
      await mouseDrag(
        page,
        rows.getByRole('listitem').filter({ hasText: issue.summary }),
        page.locator(`section[aria-label="${todo}"] [data-drop-column]`),
      );
    }
    await expect(rows.getByRole('listitem').filter({ hasText: issue.summary })).toHaveCount(0);
  }
  if (phone) await page.keyboard.press('Escape');
  // A drop at the top of the cell goes above the card there; the button adds at the end.
  await expect(cardsOf(page, todo)).toHaveText(phone ? ['Alpha', 'Beta'] : ['Beta', 'Alpha']);

  // Alpha is done; finishing the sprint carries Beta to the next one.
  const shown = await json<Issue>(api.get(`/api/v1/issues/${String(alpha.id)}`));
  await json(
    api.post(`/api/v1/boards/${id}/sprints/${String(sprint.id)}/cards/${String(alpha.id)}/move`, {
      data: { column: done.id, version: shown.version },
    }),
  );
  // A phone shows one column at a time: the first one.
  await page.reload();
  await expect(cardsOf(page, todo)).toHaveText(['Beta']);
  await page
    .getByRole('group', { name: 'Schedule' })
    .getByRole('button', { name: 'Finish' })
    .click();
  const finish = page.getByRole('dialog', { name: `Finish “${sprint.name}”?` });
  await finish.getByRole('radio', { name: 'Move to another sprint' }).check();
  await expect(finish.getByRole('combobox', { name: 'Sprint' })).toHaveValue(String(next.id));
  await finish.getByRole('button', { name: 'Finish' }).click();
  await expect(finish).toBeHidden();
  // The board goes on to the next sprint, now the current one; the finished one keeps Alpha.
  await expect(page.getByRole('button', { name: 'Sprint: Sprint next' })).toBeVisible();
  await expect(cardsOf(page, todo)).toHaveText(['Beta']);
  await page.goto(`/boards/${id}/sprints/${String(sprint.id)}`);
  await expect(cardsOf(page, todo)).toHaveCount(0);
  if (!phone) await expect(cardsOf(page, done.value.name)).toHaveText(['Alpha']);

  // Gamma joins the next sprint from its own page.
  await page.goto(`/issues/${gamma.readable_id ?? ''}`);
  const boards = page.getByRole('region', { name: 'Boards' });
  await expect(boards.getByText('Not on boards')).toBeVisible();
  await boards.getByRole('button', { name: 'Add to board' }).click();
  const add = page.getByRole('dialog', { name: 'Add to board' });
  await expect(add.getByRole('combobox', { name: 'Board' })).toHaveValue(id);
  await add.getByRole('combobox', { name: 'Sprint' }).selectOption({ label: 'Sprint next' });
  await add.getByRole('button', { name: 'Add' }).click();
  await expect(boards.getByRole('link', { name: 'Sprint next' })).toBeVisible();
  await boards.getByRole('link', { name: 'Sprint next' }).click();
  await expect(cardsOf(page, todo)).toHaveText(['Beta', 'Gamma']);
});

test('BRD-028, BRD-029: a query board shows the changes before an add', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const key = `Q${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  await signIn(page);
  const { board, issues } = await templateProject(page, key, 'kanban', ['Delta']);
  const [delta] = issues;
  if (!delta) throw new Error('no issue');
  const id = String(board.id);
  // Its own backlog: the template's one names the board, and `board:` does not know query boards.
  const search = await json<{ id: number }>(
    page.request.post('/api/v1/saved-searches', {
      data: { name: `Backlog ${key}`, query: `project: ${key} is: unresolved` },
    }),
    201,
  );
  await json(
    page.request.patch(`/api/v1/boards/${id}`, {
      data: { mode: 'query', query: 'assignee: me', backlog: search.id, version: board.version },
    }),
  );

  await page.goto(`/boards/${id}`);
  const rows = await openBacklog(page);
  await expect(rows.getByRole('listitem').filter({ hasText: 'Delta' })).toBeVisible();
  const label = `Add ${delta.readable_id ?? ''} to the board`;
  await page.getByRole('button', { name: label }).click();
  const preview = page.getByRole('dialog', { name: `${label}?` });
  await expect(preview.getByText(/^Assignee: empty → /)).toBeVisible();
  await preview.getByRole('button', { name: 'Add' }).click();
  await expect(preview).toBeHidden();
  if (phone) await page.keyboard.press('Escape');
  const first = board.columns[0]?.value.name ?? '';
  await expect(cardsOf(page, first)).toHaveText(['Delta']);
});
