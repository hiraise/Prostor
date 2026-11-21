// The content of the issue page end to end (step 1.7b): a comment with a file and a mention, edited,
// deleted and restored; a file attached from the actions menu, previewed and deleted; two issues
// linked and unlinked; a subtask from the menu; and another client's field change and comment
// reaching the open page without a reload (ISS-048). Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import type { Issue, IssueAnswer, Project } from '../src/lib/api/schema.gen.ts';
import { ADMIN } from './accounts.ts';
import { fieldButton, issueAction, openFields, signIn } from './issues.ts';

const PICTURE = new URL('fixtures/avatar.png', import.meta.url).pathname;

async function createIssue(page: Page, project: number, summary: string): Promise<Issue> {
  const created = await page.request.post('/api/v1/issues', { data: { project, summary } });
  expect(created.status()).toBe(201);
  return ((await created.json()) as IssueAnswer).issue;
}

/** «Ещё» of the header, then one of its items. */
async function fromMenu(page: Page, item: string): Promise<void> {
  await page.getByRole('button', { name: 'More', exact: true }).click();
  await page.getByRole('menuitem', { name: item, exact: true }).click();
}

test('HIST-001, CNT-001, CNT-005, CNT-017, CNT-020, CNT-022, CNT-054, LNK-005, LNK-008, ISS-009, ISS-048: the content of the issue page', async ({
  page,
  playwright,
  baseURL,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `C${suffix}`;

  await signIn(page);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Issue content ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as Project;
  const main = await createIssue(page, project.id, 'Checkout button is slow');
  const other = await createIssue(page, project.id, `Payment provider timeout ${suffix}`);

  await page.goto(`/issues/${main.readable_id ?? String(main.id)}`);
  await expect(
    page.getByRole('heading', { level: 1, name: 'Checkout button is slow' }),
  ).toBeVisible();

  // CNT-001, CNT-054, CNT-017: a comment with a mention and a dropped file. The box opens
  // from its resting line; a sent comment folds it back.
  await page.getByRole('button', { name: 'Write a comment…' }).click();
  const box = page.getByRole('form', { name: 'New comment' });
  const field = box.getByRole('textbox', { name: /^Comment/ });
  await field.fill('Slow for ');
  await field.press('End');
  await field.pressSequentially('@eri');
  await page
    .getByRole('listbox', { name: 'People to mention' })
    .getByRole('option', { name: new RegExp(ADMIN.fullName) })
    .click();
  const dropped = await page.evaluateHandle(() => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(['trace'], 'trace.txt', { type: 'text/plain' }));
    return transfer;
  });
  await field.dispatchEvent('drop', { dataTransfer: dropped });
  await expect(field).toHaveValue(/\[trace\.txt\]/);
  await box.getByRole('button', { name: 'Send' }).click();
  const card = page.getByRole('article').filter({ hasText: 'Slow for' });
  await expect(card).toBeVisible();
  await expect(card.getByText(new RegExp(ADMIN.fullName)).first()).toBeVisible();
  await expect(card.getByText('trace.txt').first()).toBeVisible();
  await expect(page.getByRole('button', { name: 'Write a comment…' })).toBeVisible();

  // CNT-003, CNT-005: edit, delete and restore it.
  await card.getByRole('button', { name: 'Comment actions' }).click();
  await page.getByRole('menuitem', { name: 'Edit' }).click();
  // While edited, the card's text is in its field.
  const editor = page.getByRole('article').getByRole('textbox', { name: /^Comment/ });
  await editor.fill('Slow only on phones');
  await editor.press('Control+Enter');
  const edited = page.getByRole('article').filter({ hasText: 'Slow only on phones' });
  await expect(edited.getByText('edited')).toBeVisible();
  await edited.getByRole('button', { name: 'Comment actions' }).click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  const deleted = page.getByRole('article').filter({ hasText: 'Comment deleted' });
  await expect(deleted).toBeVisible();
  await deleted.getByRole('button', { name: 'Restore' }).click();
  await expect(page.getByRole('article').filter({ hasText: 'Slow only on phones' })).toBeVisible();

  // CNT-017, CNT-022, CNT-024: a file from the actions menu, its preview, deleting it.
  const chooser = page.waitForEvent('filechooser');
  await fromMenu(page, 'Attach files');
  await (await chooser).setFiles(PICTURE);
  const open = page.getByRole('button', { name: 'Open avatar.png' });
  await expect(open).toBeVisible();
  await open.click();
  const preview = page.getByRole('dialog', { name: 'avatar.png' });
  await expect(preview.getByRole('img', { name: 'avatar.png' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(preview).toBeHidden();
  await page.getByRole('button', { name: 'Actions with avatar.png' }).click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  await page
    .getByRole('dialog', { name: 'Delete avatar.png?' })
    .getByRole('button', { name: 'Delete' })
    .click();
  await expect(open).toBeHidden();

  // LNK-005, LNK-008: link the two issues, then remove the link (the link icon in the title row).
  await issueAction(page, 'Link');
  const dialog = page.getByRole('dialog', { name: 'Link issues' });
  await dialog.getByRole('searchbox', { name: 'Find an issue' }).fill('Payment provider');
  await dialog.getByRole('checkbox', { name: new RegExp(other.readable_id ?? '') }).click();
  await dialog.getByRole('button', { name: /^Link( 1)?$/ }).click();
  const links = page.getByRole('region', { name: /^Links/ });
  await expect(
    links.getByRole('link', { name: new RegExp(other.readable_id ?? '') }),
  ).toBeVisible();
  await links
    .getByRole('button', { name: `Remove the link to ${other.readable_id ?? ''}` })
    .click();
  await expect(links).toBeHidden();

  // ISS-048: another client changes a field and comments; the page shows both without a reload.
  const elsewhere = await playwright.request.newContext({ baseURL });
  try {
    const signedIn = await elsewhere.post('/api/v1/session', {
      data: { login: ADMIN.username, password: ADMIN.password },
    });
    expect(signedIn.status()).toBe(201);
    const priority = main.fields.find((entry) => entry.field.name === 'Priority');
    expect(priority).toBeDefined();
    const fieldId = String(priority?.field.id);
    const offered = (await (
      await elsewhere.get(`/api/v1/issues/${String(main.id)}/fields/${fieldId}/options`)
    ).json()) as { values: { id: number; name: string }[] };
    const high = offered.values.find((value) => value.name === 'High');
    expect(high).toBeDefined();
    const set = await elsewhere.put(`/api/v1/issues/${String(main.id)}/fields/${fieldId}`, {
      data: { value: high?.id },
    });
    expect(set.ok()).toBe(true);
    const commented = await elsewhere.post(`/api/v1/issues/${String(main.id)}/comments`, {
      data: { text: 'Reproduced on the staging server' },
    });
    expect(commented.status()).toBe(201);
  } finally {
    await elsewhere.dispose();
  }
  await openFields(page);
  const panel = page.getByRole('complementary', { name: 'Fields' });
  await expect(fieldButton(panel, 'Priority')).toHaveText(/High/);
  // The comment box sits above the entries, so the new comment is appended or announced.
  const shown = page.getByRole('article').filter({ hasText: 'Reproduced on the staging server' });
  const notice = page.getByRole('button', { name: 'Show', exact: true });
  await expect(shown.or(notice)).toBeVisible();
  if (await notice.isVisible()) await notice.click();
  await expect(shown).toBeVisible();

  // ISS-009: a subtask from the menu starts linked to its parent.
  await fromMenu(page, 'New subtask');
  const form = page.getByRole('form', { name: 'New issue form' });
  await form.getByRole('textbox', { name: 'Summary' }).fill('Measure the checkout time');
  await form.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(
    page.getByRole('heading', { level: 1, name: 'Measure the checkout time' }),
  ).toBeVisible();
  // The parent under the title, and in the links panel.
  const parent = page.getByRole('paragraph').filter({ hasText: 'Subtask of' });
  await expect(
    parent.getByRole('link', { name: new RegExp(main.readable_id ?? '') }),
  ).toBeVisible();
});
