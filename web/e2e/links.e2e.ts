// Emailed links end to end (ACC-004, AUTH-008): an administrator invites a person, who joins from
// the email; later the person forgets the password and resets it from another email. The emails
// go through a real SMTP server (Mailpit). Runs on a desktop and on a phone.
import { expect, test } from '@playwright/test';

import { ADMIN } from './accounts.ts';
import { freshAddress, lastMail, linkPath } from './mail.ts';

test('ACC-004, AUTH-008: join from an invitation, then reset a forgotten password', async ({
  page,
  request,
}) => {
  const email = freshAddress('invited');
  // The administrator invites through the API with a session of their own.
  const signedIn = await page.request.post('/api/v1/session', {
    data: { login: ADMIN.username, password: ADMIN.password },
  });
  expect(signedIn.status()).toBe(201);
  const invited = await page.request.post('/api/v1/users/invitations', {
    data: { emails: email },
  });
  expect(invited.status()).toBe(201);
  await page.context().clearCookies();

  // ACC-004: the invitation link opens the page to join.
  const invitation = await lastMail(request, email, 'invited you');
  await page.goto(linkPath(invitation.text, 'invitation'));
  await expect(page.getByRole('heading', { level: 1, name: 'Join TaskMaster' })).toBeVisible();
  // The secret is gone from the address bar.
  expect(new URL(page.url()).hash).toBe('');
  await page.getByLabel(/^Full name/).fill('Ivy Invited');
  await page.getByLabel(/^New password/).fill('ivy first password');
  await page.getByLabel(/^Repeat the new password/).fill('ivy first password');
  await page.getByRole('button', { name: 'Join' }).click();
  await expect(page).toHaveURL(/\/issues$/);
  await expect(page.getByRole('button', { name: 'Account: Ivy Invited' })).toBeVisible();
  await page.context().clearCookies();

  // AUTH-008: the sign-in page leads to a reset link by email.
  await page.goto('/signin');
  await page.getByRole('link', { name: 'Forgot your password?' }).click();
  await page.getByLabel(/^Email/).fill(email.toUpperCase());
  await page.getByRole('button', { name: 'Send link' }).click();
  await expect(
    page.getByRole('main').getByText('If an account has this confirmed address'),
  ).toBeVisible();
  const reset = await lastMail(request, email, 'Reset your TaskMaster password');
  await page.goto(linkPath(reset.text, 'reset-password'));
  await page.getByLabel(/^New password/).fill('ivy second password');
  await page.getByLabel(/^Repeat the new password/).fill('ivy second password');
  await page.getByRole('button', { name: 'Set password' }).click();
  await expect(page).toHaveURL(/\/issues$/);
  await page.context().clearCookies();

  // Only the new password works now.
  const old = await request.post('/api/v1/session', {
    data: { login: email, password: 'ivy first password' },
  });
  expect(old.status()).toBe(401);
  const fresh = await request.post('/api/v1/session', {
    data: { login: email, password: 'ivy second password' },
  });
  expect(fresh.status()).toBe(201);
});
