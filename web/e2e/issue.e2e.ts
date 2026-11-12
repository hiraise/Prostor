// The issue page end to end (step 1.7a): an issue opens by its readable ID and by the ID it had
// before the project's key changed; a field changes inline; the summary and description change in
// edit mode, once over a text someone else saved meanwhile; a tag, the star, and deleting. Runs on
// a desktop and on a phone.
import { expect, test } from '@playwright/test';

import type { Issue, IssueAnswer, Project } from '../src/lib/api/schema.gen.ts';
import { fieldButton, signIn } from './issues.ts';

test('ISS-001, ISS-002, ISS-003, ISS-016, ISS-018, ISS-019, ISS-024, ISS-032, ISS-034, ISS-051: the issue page', async ({
  page,
}, testInfo) => {
  const suffix = `${testInfo.project.name === 'phone' ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `I${suffix}`;
  const renamed = `J${suffix}`;
  const phone = testInfo.project.name === 'phone';

  await signIn(page);

  // A project and an issue in it, through the API with the page's session.
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Issue page ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const created = await page.request.post('/api/v1/issues', {
    data: { project: project.id, summary: 'Login button is grey', description: 'It looks off.' },
  });
  expect(created.status()).toBe(201);
  const issue = ((await created.json()) as IssueAnswer).issue;
  expect(issue.readable_id).toBe(`${key}-1`);

  // ISS-001: by the readable ID.
  await page.goto(`/issues/${key}-1`);
  await expect(page.getByRole('heading', { level: 1, name: 'Login button is grey' })).toBeVisible();

  // ISS-003: the key changes; the old ID still leads to the issue, under its new ID.
  const fresh = (await (
    await page.request.get(`/api/v1/projects/${String(project.id)}`)
  ).json()) as Project;
  const rekeyed = await page.request.patch(`/api/v1/projects/${String(project.id)}`, {
    data: { key: renamed, version: fresh.version },
  });
  expect(rekeyed.ok()).toBe(true);
  await page.goto(`/issues/${key}-1`);
  await expect(page).toHaveURL(new RegExp(`/issues/${renamed}-1$`));
  await expect(page.getByRole('heading', { level: 1, name: 'Login button is grey' })).toBeVisible();
  await expect(page.getByText(`${renamed}-1`, { exact: true })).toBeVisible();

  // ISS-019: a field changes inline and is saved at once.
  const panel = page.getByRole('complementary', { name: 'Fields' });
  await fieldButton(panel, 'Priority').click();
  await page
    .getByRole('dialog', { name: 'Change Priority' })
    .getByRole('radio', { name: 'High' })
    .click();
  await expect(fieldButton(panel, 'Priority')).toHaveText(/High/);
  await expect
    .poll(async () => {
      const now = (await (
        await page.request.get(`/api/v1/issues/${String(issue.id)}`)
      ).json()) as Issue;
      return now.fields
        .find((entry) => entry.field.name === 'Priority')
        ?.values.map((value) => value.name);
    })
    .toEqual(['High']);

  // ISS-018: edit mode changes the summary and the description together.
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await page.getByRole('textbox', { name: 'Summary' }).fill('Login button is too pale');
  await page.getByRole('textbox', { name: 'Description' }).fill('The contrast is too low.');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(
    page.getByRole('heading', { level: 1, name: 'Login button is too pale' }),
  ).toBeVisible();
  await expect(page.getByText('The contrast is too low.')).toBeVisible();

  // ISS-051: someone saves the description while we edit; saving shows both texts.
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await page.getByRole('textbox', { name: 'Description' }).fill('Mine: raise the contrast.');
  const before = (await (
    await page.request.get(`/api/v1/issues/${String(issue.id)}`)
  ).json()) as Issue;
  const theirs = await page.request.patch(`/api/v1/issues/${String(issue.id)}`, {
    data: { description: 'Theirs: use the brand colour.', text_version: before.text_version },
  });
  expect(theirs.ok()).toBe(true);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  const conflict = page.getByRole('dialog', { name: 'Someone changed this issue' });
  await expect(conflict).toBeVisible();
  await expect(conflict.getByText(/brand colour/).first()).toBeVisible();
  await conflict.getByRole('button', { name: 'Save my version' }).click();
  await expect(conflict).toBeHidden();
  await expect(page.getByText('Mine: raise the contrast.')).toBeVisible();
  await page.reload();
  await expect(page.getByText('Mine: raise the contrast.')).toBeVisible();

  // ISS-034: a new tag, put on the issue from the picker (on a phone it is under «More»).
  const tag = `pale-${suffix.toLowerCase()}`;
  if (phone) {
    await page.getByRole('button', { name: 'More' }).click();
    await page.getByRole('menuitem', { name: 'Add tag' }).click();
  } else {
    await page.getByRole('button', { name: 'Add tag' }).click();
  }
  const picker = page.getByRole('dialog', { name: 'Add tag' });
  await picker.getByRole('searchbox', { name: 'Find a tag' }).fill(tag);
  await picker.getByRole('button', { name: `Create tag “${tag}”` }).click();
  await expect(picker.getByRole('checkbox', { name: tag })).toBeChecked();
  await page.keyboard.press('Escape');
  await expect(picker).toBeHidden();
  await expect(page.getByRole('list', { name: 'Tags' }).getByText(tag)).toBeVisible();

  // ISS-032: the star is a toggle that stays after a reload. The creator watches from
  // publication (ISS-030's auto-watch), so the star starts on; a click takes it off.
  const star = page.getByRole('button', { name: 'Watch', exact: true });
  await expect(star).toHaveAttribute('aria-pressed', 'true');
  const unstarred = page.waitForResponse(
    (response) => response.request().method() !== 'GET' && response.url().endsWith('/star'),
  );
  await star.click();
  await expect(star).toHaveAttribute('aria-pressed', 'false');
  expect((await unstarred).ok()).toBe(true);
  await page.reload();
  await expect(page.getByRole('button', { name: 'Watch', exact: true })).toHaveAttribute(
    'aria-pressed',
    'false',
  );

  // ISS-024: deleted after a confirmation; the page goes to the project's issues.
  await page.getByRole('button', { name: 'More' }).click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  const remove = page.getByRole('dialog', { name: `Delete ${renamed}-1?` });
  await remove.getByRole('button', { name: 'Delete issue' }).click();
  await expect(page).toHaveURL(new RegExp(`/issues\\?project=${renamed}$`));
  await expect(
    page.getByRole('region', { name: 'Status messages' }).getByText(`${renamed}-1 deleted`),
  ).toBeVisible();
  const gone = await page.request.get(`/api/v1/issues/${String(issue.id)}`);
  expect(gone.status()).toBe(404);
});
