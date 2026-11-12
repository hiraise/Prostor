// Link types end to end (step 1.4c): an administrator creates a type, renames it, deletes it and
// restores it from the deleted types. Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import { ADMIN } from './accounts.ts';

/** Closes the confirmations: on a phone they cover the restore buttons of the last rows. */
async function dismissToasts(page: Page): Promise<void> {
  const buttons = page
    .getByRole('region', { name: 'Status messages' })
    .getByRole('button', { name: 'Dismiss' });
  while ((await buttons.count()) > 0) await buttons.first().click();
}

test('LNK-018, LNK-019, LNK-020, LNK-021: create, rename, delete and restore a link type', async ({
  page,
}, testInfo) => {
  const suffix = `${testInfo.project.name}-${String(Date.now())}`;
  const name = `Causes ${suffix}`;
  const renamed = `Leads ${suffix}`;

  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(ADMIN.username);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);

  // LNK-018: the built-in types are there; the locked ones offer no actions.
  await page.goto('/admin');
  await page
    .getByRole('navigation', { name: 'Administration' })
    .getByRole('link', { name: 'Link types' })
    .click();
  await expect(page).toHaveURL(/\/admin\/link-types$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Link types' })).toBeVisible();
  const list = page.getByRole('list', { name: 'Link types' });
  await expect(list.getByRole('listitem').filter({ hasText: /^Subtask/ })).toContainText(
    'Cannot be changed',
  );
  await expect(page.getByRole('button', { name: 'Actions for “Subtask”' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Actions for “Dependency”' })).toBeVisible();

  // LNK-019: a directed type.
  await page.getByRole('button', { name: 'Create link type' }).click();
  const create = page.getByRole('dialog', { name: 'New link type' });
  await create.getByLabel(/^Name/).fill(name);
  await create.getByLabel(/^Source side/).fill(`causes ${suffix}`);
  await create.getByLabel(/^Target side/).fill(`caused by ${suffix}`);
  await create.getByRole('button', { name: 'Create type' }).click();
  await expect(create).toBeHidden();
  const row = list.getByRole('listitem').filter({ hasText: name });
  await expect(row).toContainText(`Sides: “causes ${suffix}” and “caused by ${suffix}”`);
  await expect(row).toContainText('Directed · No links');

  // LNK-020: renamed, and without a direction. Opening the menu closes the toasts.
  await page.getByRole('button', { name: `Actions for “${name}”` }).click();
  await page.getByRole('menuitem', { name: 'Edit' }).click();
  const edit = page.getByRole('dialog', { name: `Edit “${name}”` });
  await edit.getByLabel(/^Name/).fill(renamed);
  await expect(edit.getByText(/New words apply to every link/)).toBeVisible();
  await edit.getByRole('radio', { name: 'Undirected' }).click();
  await expect(edit.getByLabel(/^Target side/)).toHaveCount(0);
  await expect(edit.getByText(/is dropped/)).toBeVisible();
  await edit.getByRole('button', { name: 'Save' }).click();
  await expect(edit).toBeHidden();
  const renamedRow = list.getByRole('listitem').filter({ hasText: renamed });
  await expect(renamedRow).toContainText(`Both sides: “causes ${suffix}”`);
  await expect(renamedRow).toContainText('Undirected');

  // LNK-021: deleted after a confirmation, then restored.
  await page.getByRole('button', { name: `Actions for “${renamed}”` }).click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  const remove = page.getByRole('dialog', { name: `Delete “${renamed}”?` });
  await expect(remove.getByText('No issue is linked with this type.')).toBeVisible();
  await remove.getByRole('button', { name: 'Delete type' }).click();
  await expect(remove).toBeHidden();
  await expect(renamedRow).toHaveCount(0);
  const trash = page.getByRole('list', { name: 'Deleted types' });
  await expect(trash.getByRole('listitem').filter({ hasText: renamed })).toBeVisible();

  await dismissToasts(page);
  await page.getByRole('button', { name: `Restore “${renamed}”` }).click();
  await expect(renamedRow).toBeVisible();
  await expect(trash.getByRole('listitem').filter({ hasText: renamed })).toHaveCount(0);

  // The page shows the same after a reload.
  await page.reload();
  await expect(list.getByRole('listitem').filter({ hasText: renamed })).toBeVisible();
});
