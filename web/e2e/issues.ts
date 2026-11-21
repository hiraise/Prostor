// Steps the issue flows share.
import { expect, type Locator, type Page } from '@playwright/test';

import { ADMIN } from './accounts.ts';

export async function signIn(page: Page): Promise<void> {
  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(ADMIN.username);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);
}

export async function openFields(page: Page): Promise<void> {
  const details = page.locator('details.fields');
  await expect(details).toBeVisible();
  const panel = details.getByRole('complementary', { name: 'Fields' });
  if (!(await panel.isVisible())) await details.locator('summary').click();
}

export async function issueAction(page: Page, name: string): Promise<void> {
  const direct = page.getByRole('button', { name, exact: true });
  if (await direct.isVisible()) {
    await direct.click();
    return;
  }
  await page.getByRole('button', { name: 'More', exact: true }).click();
  await page.getByRole('menuitem', { name, exact: true }).click();
}

/** The value button of a field in the field panel; it is named by the value it shows. */
export function fieldButton(panel: Locator, name: string): Locator {
  return panel
    .getByRole('term')
    .filter({ has: panel.page().getByText(name, { exact: true }) })
    .locator('xpath=following-sibling::dd[1]')
    .getByRole('button');
}
