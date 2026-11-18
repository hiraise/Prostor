// The command dialog end to end (step 2.1 §8): opened from the issue page's «Ещё» menu, a line
// applied with its preview (CMD-023, CMD-028), a problem that blocks the apply, silent application
// kept from those without the right (CMD-024, CMD-025), the recent group (CMD-030), a comment with
// its visibility (CMD-019), and the bulk flow with its progress (CMD-026). Runs on a desktop and on
// a phone.
import { expect, test, type Locator, type Page } from '@playwright/test';

import type { Issue, IssueAnswer, Project } from '../src/lib/api/schema.gen.ts';
import { signIn } from './issues.ts';

/** The command dialog, single («Commands») or bulk («Apply a command to N issues»). */
function commands(page: Page): Locator {
  return page.getByRole('dialog', { name: /^Commands$|^Apply a command to \d+ issues$/ });
}

async function createIssue(page: Page, project: number, summary: string): Promise<Issue> {
  const created = await page.request.post('/api/v1/issues', { data: { project, summary } });
  expect(created.status()).toBe(201);
  return ((await created.json()) as IssueAnswer).issue;
}

/** The issue's value of a named field, as the answer's `fields` carry it. */
function fieldValue(issue: Issue, name: string): string | null {
  const found = issue.fields.find((entry) => entry.field.name === name);
  return found?.values[0]?.name ?? null;
}

test('CMD-019, CMD-023, CMD-024, CMD-025, CMD-026, CMD-028, CMD-030, ISS-017: commands on issues', async ({
  page,
  browser,
  baseURL,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `K${suffix}`;
  const tag = `cmd-${suffix.toLowerCase()}`;

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Commands ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const tagged = await page.request.post('/api/v1/tags', { data: { name: tag } });
  expect(tagged.status()).toBe(201);
  const one = await createIssue(page, project.id, 'First widget');
  const two = await createIssue(page, project.id, 'Second widget');
  const three = await createIssue(page, project.id, 'Third widget');

  // A project member (PRJ-048): the «Project member» role lets her read and change the project's
  // issues but not run commands silently (CMD-024), and the audience picker offers her.
  const login = `lea-${suffix.toLowerCase()}`;
  const account = await page.request.post('/api/v1/users', {
    data: {
      username: login,
      full_name: 'Lea Lim',
      email: `${login}@example.com`,
      password: 'lea quiet password',
    },
  });
  expect(account.status()).toBe(201);
  const lea = (await account.json()) as { id: number };
  const joined = await page.request.post(`/api/v1/projects/${String(project.id)}/team`, {
    data: { users: [lea.id] },
  });
  expect(joined.status()).toBe(200);

  // ISS-017: the dialog opens from the issue page's «Ещё» menu.
  await page.goto(`/issues/${key}-1`);
  await page.getByRole('button', { name: 'More' }).click();
  await page.getByRole('menuitem', { name: 'Commands…' }).click();
  const dialog = commands(page);
  await expect(dialog).toBeVisible();
  const input = dialog.getByRole('combobox', { name: 'Command' });

  // CMD-028: the preview lists the operations; an unknown value is a problem that blocks the
  // apply until the line is fixed.
  await input.fill(`priority Nope tag ${tag}`);
  await expect(dialog.getByText('No such value in “Priority”')).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Apply', exact: true })).toBeDisabled();
  await input.fill(`priority High tag ${tag}`);
  const preview = dialog.getByRole('region', { name: 'Preview' });
  if (phone) {
    // The fold's button names the count of what the assist answered; a click lays it open.
    const fold = preview.getByRole('button', { name: /change\(s\)/ });
    await expect(fold).toBeVisible();
    await fold.click();
  }
  await expect(preview.getByText('High')).toBeVisible();
  await expect(preview.getByText(tag)).toBeVisible();

  // CMD-023, CMD-025: applied with the buttons; silent stays for those with the right.
  await expect(dialog.getByRole('button', { name: 'Apply without notifications' })).toBeVisible();
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(
    page.getByRole('region', { name: 'Status messages' }).getByText('Command applied to 1'),
  ).toBeVisible();
  await expect
    .poll(async () => {
      const now = (await (
        await page.request.get(`/api/v1/issues/${String(one.id)}`)
      ).json()) as Issue;
      return { priority: fieldValue(now, 'Priority'), tags: now.tags.map((t) => t.name) };
    })
    .toEqual({ priority: 'High', tags: [tag] });

  // CMD-019: a comment with its visibility; the recent group offers the last applied line
  // (CMD-030) — the dialog reopens with it, and the emptied line finds it among the suggestions.
  await page.getByRole('button', { name: 'More' }).click();
  await page.getByRole('menuitem', { name: 'Repeat last command' }).click();
  await expect(commands(page).getByRole('combobox', { name: 'Command' })).toHaveValue(
    `priority High tag ${tag}`,
  );
  const reopened = commands(page);
  await reopened.getByRole('combobox', { name: 'Command' }).fill('');
  await page.keyboard.press('ArrowDown');
  await expect(
    reopened.getByRole('option', { name: new RegExp(`priority High tag ${tag}`) }),
  ).toBeVisible();
  await reopened.getByRole('combobox', { name: 'Command' }).fill('comment');
  await reopened.getByRole('textbox', { name: 'Comment' }).fill('Done in passing.');
  await reopened.getByRole('radio', { name: 'Only the chosen' }).check();
  await reopened.getByRole('searchbox', { name: 'Find a group or a person' }).fill('Lea');
  await reopened.getByRole('checkbox', { name: /Lea Lim/ }).check();
  await reopened.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(reopened).toBeHidden();
  // The comment shows on the issue, with the restriction naming its audience (CNT-003).
  await page.reload();
  await expect(page.getByText('Done in passing.')).toBeVisible();
  await expect(page.getByText(/Visible to .*Lea Lim/)).toBeVisible();

  // CMD-024, CMD-025: without `issue.silent` the quiet button never shows; the line still applies.
  const other = await browser.newContext({ baseURL });
  try {
    const theirs = await other.newPage();
    await theirs.goto('/signin');
    await theirs.getByLabel(/^Username or email/).fill(login);
    await theirs.getByLabel(/^Password/).fill('lea quiet password');
    await theirs.getByRole('button', { name: 'Sign in' }).click();
    await expect(theirs).toHaveURL(/\/issues$/);
    await theirs.goto(`/issues/${key}-2`);
    await theirs.getByRole('button', { name: 'More' }).click();
    await theirs.getByRole('menuitem', { name: 'Commands…' }).click();
    const leaDialog = commands(theirs);
    await expect(leaDialog).toBeVisible();
    await expect(
      leaDialog.getByRole('button', { name: 'Apply without notifications' }),
    ).toBeHidden();
    await leaDialog.getByRole('combobox', { name: 'Command' }).fill('priority High');
    await leaDialog.getByRole('button', { name: 'Apply', exact: true }).click();
    await expect(leaDialog).toBeHidden();
    await expect(
      theirs.getByRole('region', { name: 'Status messages' }).getByText('Command applied to 1'),
    ).toBeVisible();
  } finally {
    await other.close();
  }

  // CMD-026: the list's settings menu starts the bulk flow; the progress view follows the job.
  await page.goto(`/issues?project=${key}`);
  await page.getByRole('button', { name: 'List settings' }).click();
  await page.getByRole('menuitem', { name: 'Apply a command to all 3 issues…' }).click();
  const bulk = page.getByRole('dialog', { name: 'Apply a command to 3 issues' });
  await expect(bulk).toBeVisible();
  await bulk.getByRole('combobox', { name: 'Command' }).fill('priority High');
  await bulk.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(bulk.getByText('Done: applied to 3')).toBeVisible({ timeout: 30_000 });
  // The once-only prompt about silent-by-default bulk commands (§6).
  const prompt = page.getByRole('dialog', {
    name: 'Apply commands to lists without notifications?',
  });
  await expect(prompt).toBeVisible();
  await prompt.getByRole('button', { name: 'No', exact: true }).click();
  await expect(prompt).toBeHidden();
  for (const issue of [one, two, three]) {
    const now = (await (
      await page.request.get(`/api/v1/issues/${String(issue.id)}`)
    ).json()) as Issue;
    expect(fieldValue(now, 'Priority')).toBe('High');
  }
});
