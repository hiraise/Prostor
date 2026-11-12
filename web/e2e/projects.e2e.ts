// Projects end to end (steps 1.2–1.4a): an administrator creates a project with the wizard, writes
// its description in Markdown and ticks its checklist on the overview, gets the default fields, adds a field, deletes a value an issue holds and puts another one instead,
// adds a person to the team, and archives and restores the project. Runs on a desktop and on a
// phone.
import { expect, test } from '@playwright/test';

import type { Issue, Project, ProjectFields, ValueSet } from '../src/lib/api/schema.gen.ts';
import { ADMIN } from './accounts.ts';

test('PRJ-017, PRJ-019, PRJ-006, CNT-041, CNT-045, PRJ-022, PRJ-048, FLD-052, FLD-032, PRJ-061, PRJ-063: a project from the wizard to the archive', async ({
  page,
}, testInfo) => {
  const suffix = `${testInfo.project.name === 'phone' ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const name = `Website ${suffix}`;
  const key = `W${suffix}`;

  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(ADMIN.username);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);

  // PRJ-017: the wizard suggests a key; we type our own.
  await page.goto('/projects/new');
  await page.getByText('Scrum', { exact: true }).click();
  await expect(page.getByRole('radio', { name: 'Scrum' })).toBeChecked();
  await page.getByLabel(/^Name/).fill(name);
  await page.getByLabel(/^Key/).fill(key);
  await page.getByRole('button', { name: 'Create project' }).click();
  await expect(page).toHaveURL(new RegExp(`/projects/${key}$`));
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible();

  // PRJ-006: a Markdown description with a heading, a checklist and a code block.
  await page.goto(`/projects/${key}/settings/general`);
  await page
    .getByRole('textbox', { name: 'Description' })
    .fill('## Goals\n\n- [ ] Ship the site\n- [ ] Write the docs\n\n```rust\nfn main() {}\n```');
  await page.getByRole('tab', { name: 'Preview' }).click();
  await expect(page.getByRole('heading', { name: 'Goals' })).toBeVisible();
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(
    page.getByRole('region', { name: 'Status messages' }).getByText('Settings saved'),
  ).toBeVisible();

  // The overview renders it; the code is highlighted (the lazy chunk passes the CSP).
  await page.goto(`/projects/${key}`);
  const description = page.getByRole('region', { name: 'Description' });
  await expect(description.getByRole('heading', { level: 4, name: 'Goals' })).toBeVisible();
  await expect(description.getByRole('region', { name: 'Code, rust' })).toBeVisible();
  await expect(description.locator('.hljs-keyword').first()).toHaveText('fn');

  // CNT-041: a ticked box is saved into the text and stays ticked after a reload.
  const ship = description.getByRole('checkbox', { name: 'Ship the site' });
  await expect(ship).not.toBeChecked();
  const saved = page.waitForResponse(
    (response) => response.request().method() === 'PATCH' && response.ok(),
  );
  await ship.click();
  await saved;
  await page.reload();
  await expect(description.getByRole('checkbox', { name: 'Ship the site' })).toBeChecked();
  await expect(description.getByRole('checkbox', { name: 'Write the docs' })).not.toBeChecked();

  // PRJ-022, FLD-070: the template's fields are there.
  await page.goto(`/projects/${key}/settings/fields`);
  await expect(page.getByRole('main').getByText('Sprints').first()).toBeVisible();

  // FLD-052: a new field for this project.
  await page.getByRole('button', { name: 'Add field' }).click();
  const add = page.getByRole('dialog');
  await add.getByText('A new field', { exact: true }).click();
  await expect(add.getByRole('radio', { name: 'A new field' })).toBeChecked();
  await add.getByLabel(/^Name/).fill(`Customer ${suffix}`);
  await add.getByLabel(/^Type/).selectOption({ label: 'Choice' });
  await add.getByRole('button', { name: 'Create field' }).click();
  await expect(page.getByRole('button', { name: `Customer ${suffix}`, exact: true })).toBeVisible();

  // FLD-032: two values in its set and an issue holding one of them, through the API with the
  // page's session.
  const project = (await (
    await page.request.get(`/api/v1/projects/by-key/${key}`)
  ).json()) as Project;
  const fields = (await (
    await page.request.get(`/api/v1/projects/${String(project.id)}/fields`)
  ).json()) as ProjectFields;
  const customer = fields.items.find((item) => item.field.name === `Customer ${suffix}`);
  const setId = customer?.set?.id ?? 0;
  expect(setId).toBeGreaterThan(0);
  const valuesUrl = `/api/v1/value-sets/${String(setId)}/values`;
  expect((await page.request.post(valuesUrl, { data: { name: 'Acme' } })).status()).toBe(200);
  const set = (await (
    await page.request.post(valuesUrl, { data: { name: 'Globex' } })
  ).json()) as ValueSet;
  const acme = set.values.find((item) => item.name === 'Acme')?.id ?? 0;
  const created = await page.request.post('/api/v1/issues', {
    data: {
      project: project.id,
      summary: 'Invoice is late',
      fields: [{ field: customer?.field.id, value: acme }],
    },
  });
  expect(created.status()).toBe(201);
  const issue = ((await created.json()) as { issue: Issue }).issue;

  // The confirmation counts the issue; it gets the chosen value instead.
  await page.goto(`/admin/value-sets/${String(setId)}`);
  await page.getByRole('button', { name: 'Actions for “Acme”' }).click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  const confirm = page.getByRole('dialog', { name: 'Delete “Acme”?' });
  await expect(confirm.getByText(/^1 issue holds this value\./)).toBeVisible();
  await confirm.getByRole('combobox', { name: 'Put instead' }).selectOption({ label: 'Globex' });
  await confirm.getByRole('button', { name: 'Delete value' }).click();
  await expect(confirm).toBeHidden();
  await expect(page.getByRole('button', { name: 'Actions for “Acme”' })).toBeHidden();
  await expect
    .poll(async () => {
      const now = (await (
        await page.request.get(`/api/v1/issues/${String(issue.id)}`)
      ).json()) as Issue;
      const held = now.fields.find((item) => item.field.id === customer?.field.id);
      return held?.values.map((item) => item.name);
    })
    .toEqual(['Globex']);

  // PRJ-048: a new account joins the team.
  await page.goto('/admin/users');
  await page.getByRole('button', { name: 'Create account' }).click();
  const create = page.getByRole('dialog', { name: 'New account' });
  await create.getByLabel(/^Full name/).fill(`Tom ${suffix}`);
  await create.getByLabel(/^Email/).fill(`tom-${suffix.toLowerCase()}@example.com`);
  await create.getByLabel(/^Password/).fill('tom first password');
  await create.getByLabel(/^Repeat the password/).fill('tom first password');
  await create.getByRole('button', { name: 'Create account' }).click();
  await expect(page).toHaveURL(/\/users\/\d+$/);

  await page.goto(`/projects/${key}/settings/people`);
  await page.getByRole('button', { name: 'Add people' }).click();
  const people = page.getByRole('dialog');
  await people.getByLabel(/^Find people/).fill(`Tom ${suffix}`);
  await people.getByRole('checkbox', { name: new RegExp(`Tom ${suffix}`) }).check();
  await people.getByRole('button', { name: /^Add/ }).last().click();
  await expect(page.getByRole('main').getByText(`Tom ${suffix}`).first()).toBeVisible();

  // PRJ-061, PRJ-063: archive and restore.
  await page.goto(`/projects/${key}/settings/general`);
  await page.getByRole('button', { name: 'Archive project' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Archive project' }).click();
  await expect(page.getByText(/This project is archived/)).toBeVisible();
  await page.getByRole('button', { name: 'Restore project' }).first().click();
  await expect(page.getByText(/This project is archived/)).toBeHidden();
});
