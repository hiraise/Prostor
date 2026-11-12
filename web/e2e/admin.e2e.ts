// The administration area end to end (step 1.1): an administrator creates an account, a group
// with a role and adds the account to it; the new person chooses their own password at the first
// sign-in and gets what the group gives. Runs on a desktop and on a phone. Step 1.11 adds the
// capabilities screen, the jobs page and the boards of a field.
import { expect, test } from '@playwright/test';

import type { PageField, Project } from '../src/lib/api/schema.gen.ts';

import { ADMIN } from './accounts.ts';
import { signIn } from './issues.ts';

test('ACC-003, ACC-023, ACC-026, ACC-060, AUTH-010: an account, a group and a role', async ({
  page,
}, testInfo) => {
  const suffix = `${testInfo.project.name}-${String(Date.now())}`;
  const username = `dana-${suffix}`;
  const groupName = `Recruiters ${suffix}`;

  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(ADMIN.username);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);

  // ACC-003: a new account that must choose its own password (AUTH-010).
  await page.goto('/admin');
  await expect(page).toHaveURL(/\/admin\/users$/);
  await page.getByRole('button', { name: 'Create account' }).click();
  const create = page.getByRole('dialog', { name: 'New account' });
  await create.getByLabel(/^Full name/).fill('Dana Doe');
  await create.getByLabel(/^Email/).fill(`${username}@example.com`);
  await expect(create.getByLabel(/^Username/)).toHaveValue(username);
  await create.getByLabel(/^Password/).fill('dana first password');
  await create.getByLabel(/^Repeat the password/).fill('dana first password');
  await create.getByRole('button', { name: 'Create account' }).click();
  await expect(page).toHaveURL(/\/users\/\d+$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Dana Doe' })).toBeVisible();

  // ACC-023: a group; ACC-060: it holds a role; ACC-026: Dana joins it.
  await page.goto('/admin/groups');
  await page.getByRole('button', { name: 'Create group' }).click();
  const newGroup = page.getByRole('dialog', { name: 'New group' });
  await newGroup.getByLabel(/^Name/).fill(groupName);
  await newGroup.getByRole('button', { name: 'Create group' }).click();
  await expect(page).toHaveURL(/\/admin\/groups\/\d+$/);

  await page.getByRole('button', { name: 'Assign role' }).click();
  const assign = page.getByRole('dialog', { name: `Assign a role to “${groupName}”` });
  await assign.getByLabel(/^Role/).selectOption({ label: 'Account creator' });
  await assign.getByRole('button', { name: 'Assign role' }).click();
  await expect(page.getByRole('main').getByText('Account creator').first()).toBeVisible();

  await page.getByRole('button', { name: 'Add members' }).click();
  const add = page.getByRole('dialog', { name: `Add members to “${groupName}”` });
  await add.getByLabel(/^Find people/).fill(username);
  await add.getByRole('checkbox', { name: /Dana Doe/ }).check();
  await add.getByRole('button', { name: 'Add members' }).click();
  await expect(page.getByRole('main').getByRole('link', { name: 'Dana Doe' })).toBeVisible();

  // AUTH-010: Dana signs in and chooses a new password before anything else.
  await page.context().clearCookies();
  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(username);
  await page.getByLabel(/^Password/).fill('dana first password');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/change-password/);
  await page.getByLabel(/^Current password/).fill('dana first password');
  await page.getByLabel(/^New password/).fill('dana own password');
  await page.getByLabel(/^Repeat the new password/).fill('dana own password');
  await page.getByRole('button', { name: 'Set password' }).click();
  await expect(page).toHaveURL(/\/issues$/);

  // The group's role reaches Dana: she may create accounts now.
  await page.goto('/admin/users');
  await expect(page.getByRole('button', { name: 'Create account' })).toBeVisible();
});

test('ACC-064, ADM-031, BRD-022: a capability save, the jobs page, a field’s boards', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const stamp = `C${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  await signIn(page);

  const api = page.request;
  // A project with a kanban board (its State field gets a board) and a project whose deletion
  // becomes the job the jobs page lists.
  const boardProject = (await (
    await api.post('/api/v1/projects', {
      data: { name: `Boards ${stamp}`, key: `B${stamp}`, template: 'blank' },
    })
  ).json()) as Project;
  const madeBoard = await api.post('/api/v1/boards', {
    data: { preset: 'kanban', name: `Kanban ${stamp}`, projects: [boardProject.id] },
  });
  expect(madeBoard.status()).toBe(201);
  const doomed = (await (
    await api.post('/api/v1/projects', {
      data: { name: `Doomed ${stamp}`, key: `D${stamp}`, template: 'blank' },
    })
  ).json()) as Project;
  const deletion = await api.delete(`/api/v1/projects/${String(doomed.id)}`);
  expect(deletion.status()).toBe(202);

  // ADM-031: the jobs page opens with the deletion among its rows.
  await page.goto('/admin/jobs');
  await expect(page.getByRole('heading', { level: 1, name: 'Jobs' })).toBeVisible();
  const row = page.getByRole('listitem').filter({ hasText: 'Project deletion' });
  await expect(row.first()).toBeVisible();
  // The free-text filter with a leading minus excludes it (other jobs may remain listed).
  await page.getByRole('searchbox', { name: 'Filter jobs' }).fill('-project');
  await expect(row).toHaveCount(0);
  await page.getByRole('searchbox', { name: 'Filter jobs' }).fill('');

  // ACC-064: the capabilities screen saves a group change of a capability.
  const madeGroup = await api.post('/api/v1/groups', { data: { name: `Designers ${stamp}` } });
  expect(madeGroup.status()).toBe(201);
  await page.goto('/admin/capabilities');
  await expect(page.getByRole('heading', { level: 2, name: 'Sharing settings' })).toBeVisible();
  await page.getByRole('searchbox', { name: 'Find groups' }).fill(`Designers ${stamp}`);
  await page.getByRole('checkbox', { name: `Designers ${stamp}` }).check();
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(
    page.getByRole('region', { name: 'Status messages' }).getByText('Capability saved'),
  ).toBeVisible();

  // BRD-022: the State field's page lists the board that uses it as columns (the runs share one
  // database, so other boards are listed too — this one is told by its name).
  const { items } = (await (await api.get('/api/v1/fields')).json()) as PageField;
  const state = items.find((field) => field.kind === 'state');
  if (!state) throw new Error('the instance has no state field');
  await page.goto(`/admin/fields/${String(state.id)}`);
  const boardRow = page
    .getByRole('list', { name: 'Boards using “State”' })
    .getByRole('listitem')
    .filter({ hasText: `Kanban ${stamp}` });
  await expect(boardRow.getByRole('link', { name: `Kanban ${stamp}` })).toBeVisible();
  await expect(boardRow.getByText('columns')).toBeVisible();
});
