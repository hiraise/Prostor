// Notifications end to end (step 1.10b, plan §6.3): watching an issue and a subscription on a
// saved search both reach the inbox — the badge lights up, the popup shows the change with its
// kind and the folders part the reasons. Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import type { IssueAnswer } from '../src/lib/api/schema.gen.ts';
import { ADMIN } from './accounts.ts';
import { signIn } from './issues.ts';

const TOM_PASSWORD = 'tom notices things';

/** Ends the page's session and signs in as someone else. */
async function signInAs(page: Page, username: string, password: string): Promise<void> {
  await page.request.delete('/api/v1/session');
  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(username);
  await page.getByLabel(/^Password/).fill(password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);
}

test('NTF-014, NTF-015, NTF-021, NTF-047: watching and a search subscription reach the inbox', async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === 'phone';
  const suffix = `${phone ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const key = `N${suffix}`;
  const tomName = `tom${suffix.toLowerCase()}`;

  await signIn(page);

  // A project with a second person, an issue, and a saved search over the whole project — all
  // through the API with the admin's session.
  const made = await page.request.post('/api/v1/projects', {
    data: { name: `Notifications ${suffix}`, key, template: 'blank' },
  });
  expect(made.status()).toBe(201);
  const project = (await made.json()) as { id: number };
  const account = await page.request.post('/api/v1/users', {
    data: {
      username: tomName,
      full_name: `Tom Notice ${suffix}`,
      email: `${tomName}@example.com`,
      password: TOM_PASSWORD,
    },
  });
  expect(account.status()).toBe(201);
  const tom = (await account.json()) as { id: number };
  expect(
    (
      await page.request.post(`/api/v1/projects/${String(project.id)}/team`, {
        data: { users: [tom.id] },
      })
    ).ok(),
  ).toBe(true);
  const created = await page.request.post('/api/v1/issues', {
    data: { project: project.id, summary: 'Login button does nothing' },
  });
  expect(created.status()).toBe(201);
  const issue = ((await created.json()) as IssueAnswer).issue;
  const search = await page.request.post('/api/v1/saved-searches', {
    data: { name: `Everything ${suffix}`, query: `project: ${key}` },
  });
  expect(search.status()).toBe(201);
  const saved = (await search.json()) as { id: number };

  // NTF-014: the star watches the issue — the creator's own from publication (ISS-030's
  // auto-watch); NTF-015: the counter opens who watches it.
  await page.goto(`/issues/${issue.readable_id ?? ''}`);
  const star = page.getByRole('button', { name: 'Watch', exact: true });
  await expect(star).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: '1 watcher' }).click();
  const watchers = page.getByRole('dialog', { name: 'Watchers' });
  await expect(watchers.getByText('Erin Admin')).toBeVisible();

  // NTF-021: a subscription on the search, new items only.
  const subscribed = await page.request.post('/api/v1/notifications/subscriptions', {
    data: { source: 'search', source_id: saved.id, kinds: ['created'] },
  });
  expect(subscribed.status()).toBe(201);

  // The member comments on the watched issue and files a new one that matches the search; both
  // events are due at once (only updates wait for the batching window).
  await signInAs(page, tomName, TOM_PASSWORD);
  const comment = await page.request.post(`/api/v1/issues/${String(issue.id)}/comments`, {
    data: { text: 'Looks reproducible on Chrome' },
  });
  expect(comment.status()).toBe(201);
  const fresh = await page.request.post('/api/v1/issues', {
    data: { project: project.id, summary: 'Add a dark theme' },
  });
  expect(fresh.status()).toBe(201);

  // The admin's badge lights up; the popup shows the comment on the watched issue, and the
  // subscriptions folder holds the new item (NTF-049: reasons part the folders). The desktop and
  // phone runs share the database and the admin, so each other's rows may arrive too: counts are
  // «at least», and duplicated names are taken with `.first()`.
  await signInAs(page, ADMIN.username, ADMIN.password);
  // The badge's worded name carries the count (NTF-047); the sweep has a 3-second step.
  const bell = page.getByRole('button', { name: /^Notifications/ });
  await expect(page.getByRole('button', { name: /Notifications: \d+ unread/ })).toBeVisible({
    timeout: 20_000,
  });
  await bell.click();
  const inbox = page.getByRole('dialog', { name: /^Notifications/ });
  await expect(inbox.getByText('comment').first()).toBeVisible();
  await expect(inbox.getByText('Looks reproducible on Chrome').first()).toBeVisible();
  await inbox.getByRole('button', { name: 'Subscriptions' }).click();
  // The thread's name carries the issue summary (the excerpt of a `created` row is its id line).
  await expect(inbox.getByRole('region', { name: /Add a dark theme/ }).first()).toBeVisible();
});
