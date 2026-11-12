// Step 0.5 "done when": sign in, create a token, call the API with it, sign out (AUTH-001,
// AUTH-005, AUTH-029, AUTH-030). Runs on a desktop and on a phone.
import { expect, test } from '@playwright/test';

import { ADMIN } from './accounts.ts';

test('AUTH-001, AUTH-029, AUTH-005: sign in, create a token, use it, sign out', async ({
  page,
  request,
}, testInfo) => {
  // Not signed in: the account page sends to sign-in and comes back afterwards.
  await page.goto('/account');
  await expect(page).toHaveURL(/\/signin\?next=%2Faccount$/);

  await page.getByLabel(/^Username or email/).fill(ADMIN.email.toUpperCase());
  await page.getByLabel(/^Password/).fill('wrong password');
  await page.getByRole('button', { name: 'Sign in' }).click();
  // Shown in the form (and announced to screen readers).
  await expect(
    page.getByRole('main').getByText('The username or password is not correct.'),
  ).toBeVisible();

  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/account$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();

  // A read-only token for this run.
  const name = `e2e ${testInfo.project.name}`;
  await page.getByRole('button', { name: 'Create token' }).click();
  const dialog = page.getByRole('dialog', { name: 'New API token' });
  await dialog.getByLabel(/^Name/).fill(name);
  await dialog.getByRole('button', { name: 'Create token' }).click();
  const created = page.getByRole('dialog', { name: 'Token created' });
  const secret = await created.getByLabel('Token').inputValue();
  expect(secret).toMatch(/^tfp_/);
  await created.getByRole('button', { name: 'Done' }).click();
  await expect(page.getByRole('main').getByText(name)).toBeVisible();

  // The token works on its own, without the browser's cookie.
  const me = await request.get('/api/v1/users/me', {
    headers: { Authorization: `Bearer ${secret}` },
  });
  expect(me.status()).toBe(200);
  expect(await me.json()).toMatchObject({ username: ADMIN.username, email: ADMIN.email });
  // …but cannot manage credentials.
  const tokens = await request.get('/api/v1/users/me/tokens', {
    headers: { Authorization: `Bearer ${secret}` },
  });
  expect(tokens.status()).toBe(403);

  // Sign out from the account menu.
  await page.getByRole('button', { name: `Account: ${ADMIN.fullName}` }).click();
  await page.getByRole('menuitem', { name: 'Sign out' }).click();
  await expect(page).toHaveURL(/\/signin\?reason=signed-out$/);
  await expect(page.getByRole('main').getByText('You have signed out.')).toBeVisible();

  // The session has ended; the token keeps working.
  expect((await page.request.get('/api/v1/session')).status()).toBe(401);
  const after = await request.get('/api/v1/users/me', {
    headers: { Authorization: `Bearer ${secret}` },
  });
  expect(after.status()).toBe(200);
});
