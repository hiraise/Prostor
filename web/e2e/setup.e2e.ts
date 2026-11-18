// ADM-047: a fresh installation opens the setup wizard, which creates the administrator and signs
// them in.
import { expect, test } from '@playwright/test';

import { ADMIN, SETUP_CODE } from './accounts.ts';

test('ADM-047: the first visitor sets taskmaster up and is signed in', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/setup$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Set up taskmaster' })).toBeVisible();

  await page.getByLabel(/^Setup code/).fill(SETUP_CODE.toLowerCase());
  await page.getByLabel(/^Full name/).fill(ADMIN.fullName);
  await page.getByLabel(/^Username/).fill(ADMIN.username);
  await page.getByLabel(/^Email/).fill(ADMIN.email);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByLabel(/^Repeat the password/).fill(ADMIN.password);
  // The address is prefilled with the one the browser used.
  await expect(page.getByLabel(/^Address/)).toHaveValue(new URL(page.url()).origin);
  await page.getByRole('button', { name: 'Finish setup' }).click();

  await expect(page).toHaveURL(/\/issues$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Issues' })).toBeVisible();
  await expect(page.getByRole('button', { name: `Account: ${ADMIN.fullName}` })).toBeVisible();

  // The wizard does not open again.
  await page.goto('/setup');
  await expect(page).not.toHaveURL(/\/setup/);
});
