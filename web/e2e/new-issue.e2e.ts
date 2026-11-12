// New issues and drafts end to end (step 1.7a): an issue is created from the project overview in a
// project with a required field; later forms open from the «New» menu: one closed with text keeps
// its draft, which a later form continues; one left empty leaves no draft. A dialog on a desktop,
// a page on a phone.
import { expect, test, type Page } from '@playwright/test';

import type { Issue, Project, ProjectFields } from '../src/lib/api/schema.gen.ts';
import { fieldButton, signIn } from './issues.ts';

/**
 * Opens the new-issue form from the «New» menu of the shell, or from the overview of the project
 * the page shows; answers the form and the ID of its new draft.
 */
async function openForm(page: Page, from: 'menu' | 'project' = 'menu') {
  const started = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      new URL(response.url()).pathname === '/api/v1/issues' &&
      response.status() === 201,
  );
  if (from === 'project') {
    await page.getByRole('main').getByRole('button', { name: 'New issue', exact: true }).click();
  } else {
    // Not a «New» value of a field on the page.
    await page
      .getByRole('button', { name: 'New', exact: true })
      .and(page.locator(':not(main *)'))
      .click();
    await page.getByRole('menuitem', { name: 'Issue' }).click();
  }
  const draft = ((await (await started).json()) as { issue: Issue }).issue.id;
  const form = page.getByRole('form', { name: 'New issue form' });
  await expect(form.getByRole('textbox', { name: 'Summary' })).toBeVisible();
  return { form, draft };
}

/** Leaves the form the way a person does: Esc closes the dialog, a phone goes elsewhere. */
async function leaveForm(page: Page, phone: boolean): Promise<void> {
  if (phone) {
    await page.goto('/issues');
  } else {
    await page.keyboard.press('Escape');
    await expect(page.getByRole('dialog', { name: 'New issue' })).toBeHidden();
  }
}

async function draftExists(page: Page, id: number): Promise<boolean> {
  return (await page.request.get(`/api/v1/issues/${String(id)}`)).status() === 200;
}

test('ISS-005, ISS-006, ISS-008, ISS-010, ISS-011, FLD-057: create with a required field, keep a draft, drop an empty one', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `N${suffix}`;
  const name = `New issues ${suffix}`;

  await signIn(page);

  // A project whose priority must be chosen: no default, cannot be empty.
  const made = await page.request.post('/api/v1/projects', {
    data: { name, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const fields = (await (
    await page.request.get(`/api/v1/projects/${String(project.id)}/fields`)
  ).json()) as ProjectFields;
  const priority = fields.items.find((item) => item.field.name === 'Priority');
  expect(priority).toBeDefined();
  const required = await page.request.patch(
    `/api/v1/projects/${String(project.id)}/fields/${String(priority?.field.id)}`,
    { data: { can_be_empty: false, default_values: [], version: priority?.version } },
  );
  expect(required.ok()).toBe(true);

  // ISS-005: the overview of the project opens the form in that project.
  await page.goto(`/projects/${key}`);
  const first = await openForm(page, 'project');
  const panel = first.form.getByRole('complementary', { name: 'Fields' });
  await expect(panel.getByRole('button', { name: new RegExp(name) })).toBeVisible();

  // FLD-057: creating without the required value is refused and marks the field.
  await first.form.getByRole('textbox', { name: 'Summary' }).fill('Checkout times out');
  await first.form.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(first.form.getByText('Fill in the required fields first.')).toBeVisible();
  await expect(fieldButton(panel, 'Priority')).toHaveText(/Required field/);
  await fieldButton(panel, 'Priority').click();
  await page
    .getByRole('dialog', { name: 'Change Priority' })
    .getByRole('radio', { name: 'High' })
    .click();
  await expect(fieldButton(panel, 'Priority')).toHaveText(/High/);
  await first.form.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`/issues/${key}-1$`));
  await expect(page.getByRole('heading', { level: 1, name: 'Checkout times out' })).toBeVisible();
  expect(first.draft).toBeGreaterThan(0);

  // ISS-008, ISS-010: a form closed with text keeps its draft.
  const kept = await openForm(page);
  await kept.form.getByRole('textbox', { name: 'Summary' }).fill(`Draft kept ${suffix}`);
  await expect(kept.form.getByText('Draft saved', { exact: true })).toBeVisible();
  await leaveForm(page, phone);
  expect(await draftExists(page, kept.draft)).toBe(true);

  // ISS-011: a new form continues it from the drafts menu; the new form's empty draft is gone.
  const empty = await openForm(page);
  expect(empty.draft).not.toBe(kept.draft);
  await empty.form.getByRole('button', { name: 'Drafts' }).click();
  await page.getByRole('menuitem', { name: new RegExp(`^Draft kept ${suffix} ·`) }).click();
  await expect(empty.form.getByRole('textbox', { name: 'Summary' })).toHaveValue(
    `Draft kept ${suffix}`,
  );
  await expect.poll(() => draftExists(page, empty.draft)).toBe(false);

  // «Cancel and delete» discards the continued draft.
  await empty.form.getByRole('button', { name: 'Cancel and delete' }).click();
  if (phone) await expect(page).toHaveURL(new RegExp(`/issues\\?project=${key}$`));
  else await expect(page.getByRole('dialog', { name: 'New issue' })).toBeHidden();
  await expect.poll(() => draftExists(page, kept.draft)).toBe(false);

  // ISS-010: a form closed without anything typed leaves no draft.
  const untouched = await openForm(page);
  await leaveForm(page, phone);
  await expect.poll(() => draftExists(page, untouched.draft)).toBe(false);
});
