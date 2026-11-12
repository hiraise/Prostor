// The «Mail» screen of the administration area end to end (NTF-002, NTF-004, NTF-006, NTF-007;
// plan step 1.10 §5.1): the transport is saved with its secrets sealed, a test letter leaves
// through it and reaches Mailpit, and the instance switch — off until then — turns on only after
// a successful test. The desktop run configures the transport from the environment; the phone
// run, finding settings already stored, walks the switch off and on again.
import { expect, test } from '@playwright/test';

import { ADMIN } from './accounts.ts';
import { lastMail } from './mail.ts';

test('NTF-004, NTF-007, NTF-002: the transport, a test letter, the switch', async ({
  page,
  request,
}, testInfo) => {
  const to = `mail-${testInfo.project.name}-${String(Date.now())}@example.com`;

  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(ADMIN.username);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);

  await page.goto('/admin/mail');
  const notices = page.getByRole('region', { name: 'Status messages' });
  const off = page.getByText('Email is off: no letter leaves the instance.');
  const on = page.getByRole('checkbox', { name: 'Email is on' });
  // The settings load first; which of the notices stands decides where this run starts.
  await expect(page.getByRole('group', { name: 'Mail server' })).toBeVisible();

  // The first run: the environment is the source until a server is saved here, and while it is,
  // email counts as on (D-026). The transport of the run — Mailpit, no credentials of its own —
  // goes into the settings; the server address is a secret that never comes back, only the
  // Stored mark (NTF-004). Saving it starts the switch off, environment or not (NTF-002).
  if (await page.getByText('the transport comes from the environment').isVisible()) {
    await page.getByLabel(/^Server/).fill('smtp://127.0.0.1:1025');
    await page.getByLabel(/^From/).fill('trackflow <tracker@trackflow.test>');
    await page.getByLabel('Reply-To').fill('replies@trackflow.test');
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(notices.getByText('Mail settings saved')).toBeVisible();
    await expect(page.getByText('the transport comes from the environment')).toBeHidden();
    await expect(page.getByLabel(/^Server/)).toHaveAttribute('placeholder', 'Stored');
    await expect(off).toBeVisible();
    await expect(on).not.toBeChecked();
  } else {
    // The second run: the settings stand; walk the switch off the explicit way.
    await expect(on).toBeChecked();
    await on.uncheck();
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(notices.getByText('Mail settings saved')).toBeVisible();
    await expect(off).toBeVisible();
    await expect(on).not.toBeChecked();
  }

  // A test letter through the saved transport (NTF-007): Mailpit holds what went out.
  await page.getByLabel(/^To/).fill(to);
  await page.getByRole('button', { name: 'Send a test letter' }).click();
  await expect(notices.getByText('The letter went out')).toBeVisible();
  const letter = await lastMail(request, to, 'Test message');
  expect(letter.text).toContain(`Email to ${to} arrives.`);

  // The offer appears only now; switching on ends the warning (NTF-002).
  await page.getByRole('button', { name: 'Switch email on' }).click();
  await expect(off).toBeHidden();
  await expect(on).toBeChecked();
});
