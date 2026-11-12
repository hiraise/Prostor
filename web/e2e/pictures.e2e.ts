// Pictures end to end (step 1.4b): a person uploads a photo on the account page and sees it in the
// account menu, then removes it; an administrator uploads a project logo and goes back to the
// generated one. Runs on a desktop and on a phone.
import { fileURLToPath } from 'node:url';

import { expect, test, type Locator, type Page } from '@playwright/test';

import { ADMIN } from './accounts.ts';

const PICTURE = fileURLToPath(new URL('./fixtures/avatar.png', import.meta.url));

async function signIn(page: Page): Promise<void> {
  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(ADMIN.username);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);
}

/** Clicks a button that opens the file picker and chooses the picture. */
async function choosePicture(page: Page, button: string): Promise<void> {
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: button }).click();
  await (await chooser).setFiles(PICTURE);
}

/** Waits until the picture has loaded and returns its address. */
async function loaded(picture: Locator): Promise<string> {
  await expect(picture).toBeVisible();
  await expect
    .poll(() => picture.evaluate((img: HTMLImageElement) => (img.complete ? img.naturalWidth : 0)))
    .toBeGreaterThan(0);
  return (await picture.getAttribute('src')) ?? '';
}

test('ACC-017: an uploaded photo shows in the account menu and can be removed', async ({
  page,
}) => {
  await signIn(page);
  const menu = page.getByRole('button', { name: `Account: ${ADMIN.fullName}` });
  await expect(menu.locator('img')).toHaveCount(0);

  await page.goto('/account');
  await choosePicture(page, 'Upload photo');
  await expect(
    page.getByRole('region', { name: 'Status messages' }).getByText('Photo updated.'),
  ).toBeVisible();
  // The server made a picture of it, served to the signed-in person; the menu shows the same.
  const src = await loaded(page.getByRole('main').locator('img').first());
  expect(await loaded(menu.locator('img'))).toBe(src);

  // After a reload the session brings the picture along.
  await page.reload();
  await loaded(menu.locator('img'));

  // Removing it brings the initials back, so the other profile starts from the same state.
  await page.getByRole('button', { name: 'Remove photo' }).click();
  await expect(menu.locator('img')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Remove photo' })).toHaveCount(0);
});

test('PRJ-007: a project logo is uploaded and the generated one restored', async ({
  page,
}, testInfo) => {
  const suffix = `${testInfo.project.name === 'phone' ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `L${suffix}`;
  await signIn(page);

  await page.goto('/projects/new');
  await page.getByLabel(/^Name/).fill(`Logo ${suffix}`);
  await page.getByLabel(/^Key/).fill(key);
  await page.getByRole('button', { name: 'Create project' }).click();
  await expect(page).toHaveURL(new RegExp(`/projects/${key}$`));

  await page.goto(`/projects/${key}/settings/general`);
  const logo = page.getByRole('region', { name: 'Logo' });
  await expect(logo.locator('img')).toHaveCount(0);
  await choosePicture(page, 'Upload logo');
  await expect(
    page.getByRole('region', { name: 'Status messages' }).getByText('Logo updated.'),
  ).toBeVisible();
  // The project's header shows it as well.
  const src = await loaded(logo.locator('img'));
  await expect(page.locator(`img[src="${src}"]`)).toHaveCount(2);

  await page.getByRole('button', { name: 'Restore the default' }).click();
  await expect(
    page.getByRole('region', { name: 'Status messages' }).getByText('The default logo is back.'),
  ).toBeVisible();
  await expect(logo.locator('img')).toHaveCount(0);
  await expect(page.locator(`img[src="${src}"]`)).toHaveCount(0);
});
