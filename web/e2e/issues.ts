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

/** The value button of a field in the field panel; it is named by the value it shows. */
export function fieldButton(panel: Locator, name: string): Locator {
  return panel
    .getByRole('term')
    .filter({ has: panel.page().getByText(name, { exact: true }) })
    .locator('xpath=following-sibling::dd[1]')
    .getByRole('button');
}
