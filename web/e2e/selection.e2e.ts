// The selection end to end (step 2.2 §8): checkboxes and the header's page choice, the offer
// «Выбрать все N задач по запросу» with the bulk command over everything matched, the keyboard's
// `x`, `Shift+↓` and `Ctrl/⌘+A`, «Удалить» with a confirm that names the count, and the toolbar's
// tag, assignee and link pickers (§8.3). Phones use the «Выбрать» entry and the bottom-sheet
// toolbar. Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import type { Issue, IssueAnswer, Project } from '../src/lib/api/schema.gen.ts';
import { signIn } from './issues.ts';

async function createIssues(page: Page, project: number, summaries: string[]): Promise<Issue[]> {
  const made: Issue[] = [];
  for (const summary of summaries) {
    const created = await page.request.post('/api/v1/issues', { data: { project, summary } });
    expect(created.status()).toBe(201);
    made.push(((await created.json()) as IssueAnswer).issue);
  }
  return made;
}

test('QRY-083, QRY-084, CMD-026: selection, the all-matching offer and the bulk command', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `S${suffix}`;

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Selection ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  // One page of rows (50) and one issue beyond it: the offer of §8.1 names what lies past.
  await createIssues(
    page,
    project.id,
    Array.from({ length: 51 }, (_, i) => `Widget ${String(i + 1)}`),
  );

  await page.goto(`/issues?project=${key}`);
  await expect(page.getByText('51 issues', { exact: true })).toBeVisible();

  // QRY-083: the phone's «Выбрать» turns the selection mode on; the desktop's checkboxes are there.
  if (phone) {
    await page.getByRole('button', { name: 'List settings' }).click();
    await page.getByRole('menuitem', { name: 'Select' }).click();
  }
  // The first page holds whichever fifty the order shows, so a row is matched by its pattern.
  await page
    .getByRole('checkbox', { name: /^Select \S+-\d+$/ })
    .first()
    .click();
  const bar = page.getByRole('region', { name: 'Selected issues' });
  await expect(bar).toContainText('1 issue selected');

  // The header's checkbox chooses the page — 50 of 51 (the table is a wide screen's).
  if (!phone) {
    await page.getByRole('checkbox', { name: 'Select all issues on the page' }).click();
    await expect(bar).toContainText('50 issues selected');
    // Unchecking a row keeps the rest: the half-chosen page.
    await page
      .getByRole('checkbox', { name: /^Select \S+-\d+$/ })
      .first()
      .click();
    await expect(bar).toContainText('49 issues selected');
    await page.getByRole('checkbox', { name: 'Select all issues on the page' }).click();
  }

  // QRY-083: the offer takes the query's worth; every row renders chosen.
  await bar.getByRole('button', { name: 'Select all 51 issues of the query' }).click();
  await expect(bar).toContainText('all 51 issues of the query');

  // QRY-084, CMD-026: everything matched runs as the bulk job, with its progress.
  await bar.getByRole('button', { name: 'Commands…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Apply a command to 51 issues' });
  await dialog.getByRole('combobox', { name: 'Command' }).fill('priority High');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog.getByText('Done: applied to 51')).toBeVisible({ timeout: 15_000 });
  // The 2.1 silent-by-default prompt may open over the progress; both leave.
  const prompt = page.getByRole('dialog', { name: /without notifications/i });
  if (await prompt.isVisible()) await prompt.getByRole('button', { name: 'No' }).click();
  await dialog.getByRole('button', { name: 'Back' }).first().click();

  await bar.getByRole('button', { name: 'Clear selection' }).click();
  await expect(bar).toBeHidden();
  if (phone) {
    await expect(page.getByRole('checkbox', { name: /^Select \S+-\d+$/ })).toBeHidden();
  }
});

test('QRY-083, QRY-084: keyboard selection and delete with a named count', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `T${suffix}`;

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Delete ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  await createIssues(page, project.id, ['Alpha widget', 'Beta widget', 'Gamma widget']);

  await page.goto(`/issues?project=${key}`);
  await expect(page.getByText('3 issues', { exact: true })).toBeVisible();

  const bar = page.getByRole('region', { name: 'Selected issues' });
  const count = 2;
  if (phone) {
    await page.getByRole('button', { name: 'List settings' }).click();
    await page.getByRole('menuitem', { name: 'Select' }).click();
    await page.getByRole('checkbox', { name: /Select .*-1$/ }).click();
    await page.getByRole('checkbox', { name: /Select .*-2$/ }).click();
  } else {
    // QRY-085, QRY-083: `x` toggles the focused row, `Shift+↓` extends, `Ctrl/⌘+A` the page.
    await page.getByRole('combobox', { name: 'Search query' }).click();
    await page.keyboard.press('Tab');
    await page.keyboard.press('x');
    await expect(bar).toContainText('1 issue selected');
    await page.keyboard.press('Shift+ArrowDown');
    await expect(bar).toContainText('2 issues selected');
    await page.keyboard.press('Control+a');
    await expect(bar).toContainText('3 issues selected');
    // The page's choice lets a row go again: two stay chosen, the third survives the delete.
    await page
      .getByRole('checkbox', { name: /^Select \S+-\d+$/ })
      .nth(2)
      .click();
    await expect(bar).toContainText('2 issues selected');
  }

  // QRY-084: «Удалить» asks with the count and applies the delete command to the selection.
  await bar.getByRole('button', { name: 'Delete', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Commands' });
  await expect(dialog.getByRole('combobox', { name: 'Command' })).toHaveValue('delete');
  await dialog.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(
    dialog.getByRole('heading', { name: `Delete ${String(count)} issues?` }),
  ).toBeVisible();
  await dialog.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(
    page
      .getByRole('region', { name: 'Status messages' })
      .getByText(`Command applied to ${String(count)}`),
  ).toBeVisible();

  // The rows wait for a reload (D-043); the fresh list shows the one issue that stayed.
  await page.reload();
  await expect(page.getByText('1 issue', { exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: /widget/ })).toHaveCount(1);
});

test('QRY-084: the tag, assignee and link pickers run their commands over the selection', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `K${suffix}`;

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Pickers ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const madeIssues = await createIssues(page, project.id, [
    'Alpha widget',
    'Beta widget',
    'Gamma widget',
  ]);
  const gamma = madeIssues[2];
  if (gamma === undefined) throw new Error('Gamma was not created');

  await page.goto(`/issues?project=${key}`);
  await expect(page.getByText('3 issues', { exact: true })).toBeVisible();
  if (phone) {
    await page.getByRole('button', { name: 'List settings' }).click();
    await page.getByRole('menuitem', { name: 'Select' }).click();
  }
  await page.getByRole('checkbox', { name: /Select .*-1$/ }).click();
  await page.getByRole('checkbox', { name: /Select .*-2$/ }).click();
  const bar = page.getByRole('region', { name: 'Selected issues' });
  // Tags live on the instance, so no two runs may ask for the same name; the space in the
  // name also proves the line quotes what is not one word.
  const tagName = `urgent ${suffix}`;
  // An apply over chosen IDs closes the dialog once it answers — the signal the next step
  // waits for (a toast may still be the previous apply's).
  async function applyLine(dialog: ReturnType<typeof page.getByRole>): Promise<void> {
    await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
    await expect(dialog).toBeHidden();
  }

  // «Тег»: a name nobody has is created, then applied to both (§8.3, ISS-034).
  await bar.getByRole('button', { name: 'Tag', exact: true }).click();
  const tags = page.getByRole('dialog', { name: 'Add a tag to the chosen issues' });
  await tags.getByRole('searchbox', { name: 'Find a tag' }).fill(tagName);
  await tags.getByRole('button', { name: `Create tag “${tagName}”` }).click();
  let dialog = page.getByRole('dialog', { name: 'Commands' });
  await expect(dialog.getByRole('combobox', { name: 'Command' })).toHaveValue(`tag "${tagName}"`);
  await applyLine(dialog);

  // «Исполнитель»: the line names the field and the account (§8.3).
  await bar.getByRole('button', { name: 'Assignee', exact: true }).click();
  const assignee = page.getByRole('dialog', { name: 'Choose an assignee' });
  await assignee.getByRole('searchbox', { name: 'Find a person' }).fill('Erin');
  await assignee.getByRole('radio', { name: /Erin Admin/ }).click();
  dialog = page.getByRole('dialog', { name: 'Commands' });
  await expect(dialog.getByRole('combobox', { name: 'Command' })).toHaveValue('Assignee erin');
  await applyLine(dialog);

  // «Добавить связь»: every chosen issue gets the link to the chosen target (§8.3). The
  // search may find earlier runs' issues too — the target is named by its readable ID.
  await bar.getByRole('button', { name: 'Add link', exact: true }).click();
  const link = page.getByRole('dialog', { name: 'Link the chosen issues' });
  await link.getByRole('searchbox', { name: 'Find an issue' }).fill('Gamma');
  await link.getByRole('checkbox', { name: `${gamma.readable_id ?? ''} Gamma widget` }).click();
  await link.getByRole('button', { name: 'Link', exact: true }).click();
  dialog = page.getByRole('dialog', { name: 'Commands' });
  await expect(dialog.getByRole('combobox', { name: 'Command' })).toHaveValue(
    `related to ${gamma.readable_id ?? ''}`,
  );
  await applyLine(dialog);

  // The rows wait for a reload (D-043); the first issue carries all three changes.
  await page.reload();
  await page.getByRole('link', { name: 'Alpha widget' }).click();
  await expect(page.getByText(tagName, { exact: true }).first()).toBeVisible();
  await expect(page.getByText('Erin Admin', { exact: true }).first()).toBeVisible();
  // The link names its target in the panel and in the activity's change record.
  await expect(page.getByRole('link', { name: /Gamma widget/ }).first()).toBeVisible();
});
