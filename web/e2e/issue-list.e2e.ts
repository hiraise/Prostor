// The issue list end to end (step 1.7c): a project's issues from its frame, a query with a mistake
// and then fixed, sorting by a header, a value changed in a cell, a saved search found in the
// sidebar, and another client's changes: a shown row updates in place, a new issue brings the
// «Refresh» hint (QRY-086). Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import type { Issue, IssueAnswer, Project } from '../src/lib/api/schema.gen.ts';
import { ADMIN } from './accounts.ts';
import { signIn } from './issues.ts';

async function createIssue(page: Page, project: number, summary: string): Promise<Issue> {
  const created = await page.request.post('/api/v1/issues', { data: { project, summary } });
  expect(created.status()).toBe(201);
  return ((await created.json()) as IssueAnswer).issue;
}

test('QRY-053, QRY-054, QRY-073, QRY-074, QRY-075, QRY-076, QRY-082, QRY-086, PRJ-016, UI-005: the issue list', async ({
  page,
  playwright,
  baseURL,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `L${suffix}`;
  const name = `Issue list ${suffix}`;

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const alpha = await createIssue(page, project.id, 'Alpha widget');
  const beta = await createIssue(page, project.id, 'Beta widget');
  const alphaId = alpha.readable_id ?? String(alpha.id);

  // PRJ-016, UI-005: the project's «Issues» opens the list in its context, under its crumbs.
  await page.goto(`/projects/${key}`);
  await page
    .getByRole('navigation', { name: 'Project', exact: true })
    .getByRole('link', { name: 'Issues' })
    .click();
  await expect(page).toHaveURL(new RegExp(`/issues\\?project=${key}$`));
  const crumbs = page.getByRole('navigation', { name: 'Breadcrumbs' });
  await expect(crumbs.getByRole('link', { name }).first()).toHaveAttribute(
    'href',
    `/projects/${key}`,
  );
  await expect(page.getByRole('button', { name: `Search in: ${name}` })).toBeVisible();
  await expect(page.getByText('2 issues', { exact: true })).toBeVisible();

  // QRY-074: a mistake is explained; the fixed query finds the issues.
  const query = page.getByRole('combobox', { name: 'Search query' });
  await query.fill('nosuchfield: x');
  await page.getByRole('button', { name: 'Find' }).click();
  await expect(page.getByText('The query has mistakes')).toBeVisible();
  await query.fill('widget');
  await page.getByRole('button', { name: 'Find' }).click();
  await expect(page.getByText('The query has mistakes')).toBeHidden();
  await expect(page.getByRole('link', { name: 'Alpha widget' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Beta widget' })).toBeVisible();

  // QRY-075: a click on a header sorts by it, the next one reverses (tables are for wide screens).
  if (!phone) {
    const table = page.getByRole('grid', { name: 'Found issues' });
    const firstRow = table.getByRole('row').nth(1);
    await table.getByRole('button', { name: 'Summary' }).click();
    await expect(page).toHaveURL(/sort/);
    await expect(firstRow).toContainText('Alpha widget');
    await table.getByRole('button', { name: 'Summary' }).click();
    await expect(firstRow).toContainText('Beta widget');
  }

  // QRY-076: a value changes in its cell.
  if (phone) await page.getByRole('button', { name: `More for ${alphaId}` }).click();
  await page.getByRole('button', { name: new RegExp(`Priority of ${alphaId}`) }).click();
  await page
    .getByRole('dialog', { name: 'Change Priority' })
    .getByRole('radio', { name: 'High' })
    .click();
  await expect(page.getByRole('button', { name: new RegExp(`Priority of ${alphaId}`) })).toHaveText(
    /High/,
  );

  // QRY-054, QRY-082: the search is saved and listed in the sidebar.
  await page.getByRole('button', { name: 'Save search' }).click();
  const dialog = page.getByRole('dialog', { name: 'Save search' });
  await dialog.getByRole('textbox', { name: 'Name' }).fill(`Widgets ${suffix}`);
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByRole('button', { name: `Search in: Widgets ${suffix}` })).toBeVisible();
  if (phone) await page.getByRole('button', { name: 'Lists' }).click();
  await expect(
    page
      .getByRole('region', { name: 'Saved searches' })
      .getByRole('link', { name: new RegExp(`Widgets ${suffix}`) }),
  ).toBeVisible();
  if (phone) await page.keyboard.press('Escape');

  // QRY-086: another client renames a shown issue and creates a new one.
  const elsewhere = await playwright.request.newContext({ baseURL });
  try {
    const signedIn = await elsewhere.post('/api/v1/session', {
      data: { login: ADMIN.username, password: ADMIN.password },
    });
    expect(signedIn.status()).toBe(201);
    const renamed = await elsewhere.patch(`/api/v1/issues/${String(beta.id)}`, {
      data: { summary: 'Beta widget renamed', text_version: beta.text_version },
    });
    expect(renamed.ok()).toBe(true);
    const created = await elsewhere.post('/api/v1/issues', {
      data: { project: project.id, summary: 'Gamma widget' },
    });
    expect(created.status()).toBe(201);
  } finally {
    await elsewhere.dispose();
  }
  await expect(page.getByRole('link', { name: 'Beta widget renamed' })).toBeVisible();
  await expect(page.getByText('The list has changed')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Gamma widget' })).toBeHidden();
  await page.getByRole('button', { name: 'Refresh' }).click();
  await expect(page.getByRole('link', { name: 'Gamma widget' })).toBeVisible();
  await expect(page.getByText('The list has changed')).toBeHidden();
});

test('QRY-056, QRY-058: completion and markers in the query bar', async ({ page }) => {
  const suffix = `Q${String(Date.now()).slice(-6)}`;
  const key = `L${suffix}`;

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Query bar ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  await createIssue(page, project.id, 'Login button is grey');

  await page.goto('/issues');
  const query = page.getByRole('combobox', { name: 'Search query' });
  await query.click();
  const list = page.getByRole('listbox', { name: 'Suggestions' });

  // QRY-056: the manual trigger opens the list; a partial word narrows it to fields; Tab inserts
  // the active row with its suffix. The trigger is retried until the assist's answer arrives.
  await query.fill('prior');
  await expect(async () => {
    await page.keyboard.press('Control+Space');
    await expect(list.getByRole('option', { name: /priority/i })).toBeVisible({ timeout: 2000 });
  }).toPass();
  await page.keyboard.press('Tab');
  await expect(query).toHaveValue(/^Priority $/);
  await expect(list).toBeHidden();

  // The colon makes the word an attribute; after it the field's values (QRY-056), and a click
  // takes one. Enter over the closed list searches.
  // The suffix space goes before the colon: `Priority:`, and after it the values.
  await page.keyboard.press('Backspace');
  await page.keyboard.press(':');
  await expect(async () => {
    await page.keyboard.press('Control+Space');
    await expect(list.getByRole('option', { name: /High/ })).toBeVisible({ timeout: 2000 });
  }).toPass();
  await list.getByRole('option', { name: /High/ }).click();
  await expect(query).toHaveValue(/^Priority:High $/);
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/q=Priority/);

  // QRY-058: an unknown name underlines as the assist answers, the pointer gets its message, and
  // the input's description says it.
  await query.fill('nosuchfield: x');
  await expect(query).toHaveAccessibleDescription(/no such field/);
  await query.hover({ position: { x: 14, y: 12 } });
  await expect(query).toHaveAttribute('title', 'no such field');
});
