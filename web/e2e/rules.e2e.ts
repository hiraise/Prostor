// The built-in rules end to end (steps 1.9a and 1.9b): the rules tab of a blank project lists the
// template's set and turns a rule off; a subtask's state change resolves its parent, which says so
// in its stream and to the actor; a «+1» comment counts as a vote with a toast; an assignee outside
// the issue's audience draws a warning. Then the 1.9b half: naming an original puts the duplicate's
// state to «Duplicate» and a chain collapses to its root, which the toast says, while removing the
// link reopens the issue; a scrum board assigns whoever drags an unassigned card; an issue is cloned
// from its page and, as a draft, from the page and the list. Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import type {
  BoardList,
  BoardView,
  Issue,
  IssueAnswer,
  Project,
} from '../src/lib/api/schema.gen.ts';
import { ADMIN } from './accounts.ts';
import { cardsOf, mouseDrag, touchDrag } from './boards.ts';
import { fieldButton, openFields, signIn } from './issues.ts';

const TOM_PASSWORD = 'tom rules password';

async function createIssue(page: Page, project: number, summary: string): Promise<Issue> {
  const created = await page.request.post('/api/v1/issues', { data: { project, summary } });
  expect(created.status()).toBe(201);
  return ((await created.json()) as IssueAnswer).issue;
}

test('AUT-023, AUT-094, AUT-079, AUT-063: the built-in rules of a project', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `R${suffix}`;
  const toasts = page.getByRole('region', { name: 'Status messages' });

  await signIn(page);

  // A blank project and a second person in its team, through the API with the page's session.
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Rules ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const tomName = `Tom Rules ${suffix}`;
  const account = await page.request.post('/api/v1/users', {
    data: {
      username: `tom${suffix.toLowerCase()}`,
      full_name: tomName,
      email: `tom-${suffix.toLowerCase()}@example.com`,
      password: TOM_PASSWORD,
    },
  });
  expect(account.status()).toBe(201);
  const tom = (await account.json()) as { id: number };
  const added = await page.request.post(`/api/v1/projects/${String(project.id)}/team`, {
    data: { users: [tom.id] },
  });
  expect(added.ok()).toBe(true);

  // AUT-023: the whole registry with the blank template's set on and the rest off; a rule turns
  // off and stays off after a reload, and turns back on.
  await page.goto(`/projects/${key}/settings/rules`);
  const switches = page.getByRole('switch');
  await expect(page.getByRole('switch', { name: 'Assignee visibility' })).toBeVisible();
  await expect(switches).toHaveCount(9);
  for (const name of [
    'Assignee visibility',
    '+1 votes',
    'Component owner',
    'Parent follows subtasks',
    'Subtasks inherit fix versions',
    'Fixed in build',
    'Duplicates',
    'Issue cloning',
  ]) {
    await expect(page.getByRole('switch', { name })).toHaveAttribute('aria-checked', 'true');
  }
  await expect(page.getByRole('switch', { name: 'Scrum rules' })).toHaveAttribute(
    'aria-checked',
    'false',
  );
  const cloning = page.getByRole('switch', { name: 'Issue cloning' });
  await expect(cloning).toHaveAttribute('aria-checked', 'true');
  const turnedOff = page.waitForResponse(
    (response) =>
      response.request().method() === 'PATCH' && response.url().includes('/rules') && response.ok(),
  );
  await cloning.click();
  await turnedOff;
  await expect(cloning).toHaveAttribute('aria-checked', 'false');
  await page.reload();
  await expect(page.getByRole('switch', { name: 'Assignee visibility' })).toBeVisible();
  await expect(cloning).toHaveAttribute('aria-checked', 'false');
  await cloning.click();
  await expect(cloning).toHaveAttribute('aria-checked', 'true');

  // AUT-094: the subtask's state change resolves the parent, which takes «Done» and records the
  // change as the rule's.
  const parent = await createIssue(page, project.id, 'The site is down');
  const types = (await (await page.request.get('/api/v1/link-types')).json()) as {
    items: { id: number; builtin: string | null }[];
  };
  const subtask = types.items.find((type) => type.builtin === 'subtask');
  expect(subtask).toBeDefined();
  const child = await createIssue(page, project.id, 'Restart the database');
  const linked = await page.request.post(`/api/v1/issues/${String(child.id)}/links`, {
    data: { link_type: subtask?.id, issues: [parent.id], side: 'target' },
  });
  expect(linked.ok()).toBe(true);

  await page.goto(`/issues/${child.readable_id ?? ''}`);
  const panel = page.getByRole('complementary', { name: 'Fields' });
  await openFields(page);
  await fieldButton(panel, 'State').click();
  await page
    .getByRole('dialog', { name: 'Change State' })
    .getByRole('radio', { name: 'Fixed' })
    .click();
  await openFields(page);
  await expect(fieldButton(panel, 'State')).toHaveText(/Fixed/);
  await expect(toasts.getByText(`${parent.readable_id ?? ''} moved to “Fixed”`)).toBeVisible();

  await page.goto(`/issues/${parent.readable_id ?? ''}`);
  await openFields(page);
  await expect(fieldButton(panel, 'State')).toHaveText(/Fixed/);
  // The state change and the resolution itself are both recorded as the rule's work.
  await expect(page.getByText('rule', { exact: true }).first()).toBeVisible();

  // AUT-063: an assignee the issue's audience does not list is warned about.
  const audienceIssue = await createIssue(page, project.id, 'Print the invoice twice');
  const session = (await (await page.request.get('/api/v1/session')).json()) as {
    user: { id: number };
  };
  const restricted = await page.request.patch(`/api/v1/issues/${String(audienceIssue.id)}`, {
    data: { audience: { users: [session.user.id], groups: [] } },
  });
  expect(restricted.ok()).toBe(true);
  await page.goto(`/issues/${audienceIssue.readable_id ?? ''}`);
  await openFields(page);
  await fieldButton(panel, 'Assignee').click();
  await page
    .getByRole('dialog', { name: 'Change Assignee' })
    .getByRole('radio', { name: new RegExp(tomName) })
    .click();
  await openFields(page);
  await expect(fieldButton(panel, 'Assignee')).toHaveText(new RegExp(tomName));
  await expect(toasts.getByText(`The assignee ${tomName} cannot see this issue`)).toBeVisible();

  // AUT-079: a comment that says «+1» adds the commenter's vote and says so. The commenter must
  // not be the reporter, so the team member signs in.
  const voteIssue = await createIssue(page, project.id, 'Add a dark theme');
  await page.request.delete('/api/v1/session');
  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(`tom${suffix.toLowerCase()}`);
  await page.getByLabel(/^Password/).fill(TOM_PASSWORD);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);

  await page.goto(`/issues/${voteIssue.readable_id ?? ''}`);
  const vote = page.getByRole('button', { name: 'Vote' });
  await expect(vote).toHaveAttribute('aria-pressed', 'false');
  await page.getByRole('button', { name: 'Write a comment…' }).click();
  const box = page.getByRole('form', { name: 'New comment' });
  await box.getByRole('textbox', { name: /^Comment/ }).fill('+1');
  await box.getByRole('button', { name: 'Send' }).click();
  await expect(toasts.getByText('+1 counts as a vote')).toBeVisible();
  await expect(vote).toHaveAttribute('aria-pressed', 'true');
  await expect(vote).toHaveText(/1/);
});

/** Names `original` as the original of `issue` through the page's «Ещё»: the dialog searches by
 * the summary and picks the found row by its unique readable ID (a parallel run may hold issues
 * with the same summaries in other projects; a resolved row — the first duplicate — says so). */
async function markDuplicate(page: Page, issue: Issue, original: Issue): Promise<void> {
  await page.goto(`/issues/${issue.readable_id ?? ''}`);
  await page.getByRole('button', { name: 'More', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Mark as duplicate…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Mark as duplicate' });
  await dialog.getByRole('combobox', { name: 'Link' }).selectOption({ label: 'duplicate of' });
  await dialog.getByRole('searchbox', { name: 'Find an issue' }).fill(original.summary);
  const row = new RegExp(
    `^${original.readable_id ?? ''}( \\(resolved\\))? ${original.summary.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`,
  );
  await dialog.getByRole('checkbox', { name: row }).check();
  await dialog.getByRole('button', { name: 'Link', exact: true }).click();
  await expect(dialog).toBeHidden();
}

test('AUT-071: duplicates — the state follows the link, the chain collapses, the reopen', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `U${suffix}`;
  const toasts = page.getByRole('region', { name: 'Status messages' });
  const panel = page.getByRole('complementary', { name: 'Fields' });

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Duplicates ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const original = await createIssue(page, project.id, 'The printer jams');
  const middle = await createIssue(page, project.id, 'The very same printer jams');
  const late = await createIssue(page, project.id, 'A late report of the jam');

  // Rule 4: naming an original puts the duplicate's state to «Duplicate».
  await markDuplicate(page, middle, original);
  await page.goto(`/issues/${middle.readable_id ?? ''}`);
  await openFields(page);
  await expect(fieldButton(panel, 'State')).toHaveText(/Duplicate/);

  // Rule 1: the new link is rewritten to the chain's root, which the toast names; the late
  // duplicate's state flips too (rule 4) and its panel names the root as the original.
  await markDuplicate(page, late, middle);
  await expect(
    toasts.getByText(`Duplicates now point to ${original.readable_id ?? ''}`),
  ).toBeVisible();
  await page.goto(`/issues/${late.readable_id ?? ''}`);
  await openFields(page);
  await expect(fieldButton(panel, 'State')).toHaveText(/Duplicate/);
  await expect(page.getByRole('list', { name: 'duplicate of' })).toContainText(
    original.readable_id ?? '',
  );

  // Rule 3: taking the last link off a duplicate whose state is «Duplicate» reopens it.
  await page
    .getByRole('button', { name: `Remove the link to ${original.readable_id ?? ''}` })
    .click();
  await expect(toasts.getByText('Link removed')).toBeVisible();
  await page.reload();
  await openFields(page);
  await expect(fieldButton(panel, 'State')).toHaveText(/Reopened/);
});

test('AUT-085: a scrum board assigns whoever drags an unassigned card', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `M${suffix}`;
  await signIn(page);
  const api = page.request;

  const made = await api.post('/api/v1/projects', {
    data: { name: `Scrum rules ${suffix}`, key, template: 'scrum' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const created = await api.post('/api/v1/issues', {
    data: { project: project.id, summary: 'Lift the card' },
  });
  expect(created.status()).toBe(201);
  const issue = ((await created.json()) as IssueAnswer).issue;

  // The template's board, and the issue planned into its current sprint.
  const list = (await (
    await api.get(`/api/v1/boards?project=${String(project.id)}`)
  ).json()) as BoardList;
  const board = list.items[0];
  if (!board) throw new Error('the template made no board');
  const view = (await (
    await api.get(`/api/v1/boards/${String(board.id)}/view`)
  ).json()) as BoardView;
  const sprint = view.sprint;
  if (!sprint) throw new Error('the scrum board has no sprint');
  const planned = await api.post(
    `/api/v1/boards/${String(board.id)}/sprints/${String(sprint.id)}/cards`,
    {
      data: { issue: issue.id },
    },
  );
  expect(planned.status()).toBeLessThan(300);
  const [first, second] = view.board.columns.map((column) => column.value.name);
  if (!first || !second) throw new Error('the board has fewer than two columns');

  await page.goto(`/boards/${String(board.id)}`);
  await expect(cardsOf(page, first)).toHaveText(['Lift the card']);
  const drag = phone ? touchDrag : mouseDrag;
  if (phone) await page.getByRole('button', { name: 'All', exact: true }).click();
  await drag(
    page,
    cardsOf(page, first).filter({ hasText: 'Lift the card' }),
    page.locator(`section[aria-label="${second}"] [data-drop-column]`),
  );

  // The moved card carries the mover as its assignee.
  await expect(cardsOf(page, second)).toHaveText(['Lift the card']);
  const moved = page.locator(`section[aria-label="${second}"] article`).filter({
    hasText: 'Lift the card',
  });
  await expect(moved.getByRole('img', { name: `Assignee: ${ADMIN.fullName}` })).toBeVisible();
});

test('AUT-066: cloning from the page and the list', async ({ page }, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `C${suffix}`;
  const toasts = page.getByRole('region', { name: 'Status messages' });

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Cloning ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const source = await createIssue(page, project.id, 'Ship the lantern');
  const id = source.readable_id ?? '';

  // «Ещё» of the page: a published copy; the toast's «Открыть» leads to it, and its stream names
  // the original.
  await page.goto(`/issues/${id}`);
  await page.getByRole('button', { name: 'More', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Clone', exact: true }).click();
  await expect(toasts.getByText(/Copy created: /)).toBeVisible();
  await toasts.getByRole('button', { name: 'Open' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toContainText('(copy)');
  await expect(page.getByText('Created the issue by cloning')).toBeVisible();

  // «Клонировать как черновик» of the page keeps the copy among the actor's drafts.
  await page.goto(`/issues/${id}`);
  await page.getByRole('button', { name: 'More', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Clone as draft' }).click();
  await expect(toasts.getByText(`Draft copied from ${id}`)).toBeVisible();

  // The list offers the same: the row's menu on a desktop, the card's «Ещё» on a phone. The list
  // opens in the project's context — a parallel run of the other profile fills the shared one.
  await page.goto(`/issues?project=${key}`);
  if (phone) {
    await page.getByRole('button', { name: `More for ${id}` }).click();
    await page.getByRole('button', { name: 'Clone as draft' }).click();
  } else {
    const row = page.getByRole('row').filter({ hasText: id });
    await row.hover();
    await row.getByRole('button', { name: `Actions for ${id}` }).click();
    await page.getByRole('menuitem', { name: 'Clone as draft' }).click();
  }
  await expect(toasts.getByText(`Draft copied from ${id}`)).toBeVisible();

  const drafts = (await (await page.request.get('/api/v1/issues/drafts')).json()) as {
    items: Issue[];
  };
  // The two copies of this run's project (the phone and desktop runs share the actor's drafts).
  expect(
    drafts.items.filter(
      (draft) => draft.project.id === project.id && draft.summary.includes('(copy)'),
    ),
  ).toHaveLength(2);
});
