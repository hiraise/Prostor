// The «Настройки» screen of the administration area end to end (plan step 1.11 §4, §7.2;
// ADM-002, ADM-003, ADM-016): the address of the instance is set and warns that it differs from
// where this page came from, and the date format is switched and saved, with an issue list
// showing it. Both runs share one database: the desktop run stores the address, the phone run
// finds it stored and walks the warning; each run saves its own date format.
import { expect, test } from '@playwright/test';

import { ADMIN } from './accounts.ts';

test.beforeEach(async ({ page }) => {
  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(ADMIN.username);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);
  await page.goto('/admin/settings');
  await expect(page.getByRole('tab', { name: 'Server' })).toBeVisible();
});

test('ADM-003: the instance address warns while it differs from this page', async ({ page }) => {
  const address = page.getByLabel(/^Instance address/);

  // The setup wizard stored the address this page came from, so nothing warns yet; the phone run
  // finds the desktop run's address stored instead and the warning already up.
  const stored = await address.inputValue();
  const origin = new URL(page.url()).origin;
  if (stored === '' || stored === origin) {
    await address.fill('https://track.example.com');
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(
      page.getByText('letters and invitations now link to the new address;'),
    ).toBeVisible();
  }
  await expect(page.getByText(/This differs from the address this page came from/)).toBeVisible();

  // An http address warns about itself; an https one stores quietly.
  await address.fill('http://plain.example.com');
  await expect(page.getByText(/The address starts with http:/)).toBeVisible();
  await address.fill('https://track.example.com');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText(/This differs from the address this page came from/)).toBeVisible();
});

test('ADM-016: the date format is switched and saved', async ({ page }, testInfo) => {
  await page.getByRole('tab', { name: 'Localization' }).click();

  // The four variants with their samples on one screen; each run picks its own.
  await expect(page.getByText('31.12.2000, 23:59')).toBeVisible();
  await expect(page.getByText('2000-12-31 23:59')).toBeVisible();
  const pick = testInfo.project.name === 'desktop' ? 'Year first (ISO)' : 'Day first, numbers';
  await page.getByRole('radio', { name: pick }).check();
  await page.getByRole('button', { name: 'Save' }).click();
  const notices = page.getByRole('region', { name: 'Status messages' });
  await expect(notices.getByText('Settings saved')).toBeVisible();

  // The stored format reaches the dates people see: an issue list shows a date field in it. The
  // runs share one database, so each makes its own project; a past year keeps the full pattern.
  // The new field lands after the three defaults every project carries, so the list's columns are
  // chosen here to put it next to the summary.
  const desktop = testInfo.project.name === 'desktop';
  const stamp = Date.now().toString(36);
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Format ${stamp}`, key: `F${stamp}`, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as { id: number };
  const attached = await page.request.post(`/api/v1/projects/${String(project.id)}/fields`, {
    data: { source: 'new', kind: 'date', name: `Due ${stamp}` },
  });
  expect(attached.status()).toBe(201);
  const due = ((await attached.json()) as { field: { id: number } }).field.id;
  const issue = await page.request.post('/api/v1/issues', {
    data: {
      project: project.id,
      summary: `Spells its date ${stamp}`,
      fields: [{ field: due, value: '2020-05-31' }],
    },
  });
  expect(issue.status()).toBe(201);
  const columns = await page.request.patch('/api/v1/users/me/preferences', {
    data: { list_columns: ['id', 'summary', `field:${String(due)}`] },
  });
  expect(columns.status()).toBe(200);
  await page.goto(`/issues?project=F${stamp}`);
  await expect(page.getByText(desktop ? '2020-05-31' : '31.05.2020')).toBeVisible();
  // The runs share one database and one administrator: give the later tests their default
  // columns back.
  const restored = await page.request.patch('/api/v1/users/me/preferences', {
    data: { list_columns: null },
  });
  expect(restored.status()).toBe(200);
});
