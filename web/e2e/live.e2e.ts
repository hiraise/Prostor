// The live connection end to end (step 1.6): a signed-in tab opens one socket (allowed by the
// page's CSP), follows the `instance` topic, and loads permissions and settings again when the
// server says they changed or after a reconnect. Runs on a desktop and on a phone.
import { expect, test, type Page } from '@playwright/test';

import { ADMIN } from './accounts.ts';

const LIVE = '/api/v1/live';

async function signIn(page: Page): Promise<void> {
  await page.goto('/signin');
  await page.getByLabel(/^Username or email/).fill(ADMIN.username);
  await page.getByLabel(/^Password/).fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/issues$/);
}

type Message = Record<string, unknown>;

/** Every message the page's live sockets receive, collected from the moment each one opens. */
function collectMessages(page: Page): Message[] {
  const messages: Message[] = [];
  page.on('websocket', (socket) => {
    if (!socket.url().endsWith(LIVE)) return;
    socket.on('framereceived', ({ payload }) => {
      if (typeof payload === 'string') messages.push(JSON.parse(payload) as Message);
    });
  });
  return messages;
}

function subscribedToInstance(message: Message): boolean {
  return (
    message.type === 'subscribed' &&
    Array.isArray(message.topics) &&
    message.topics.includes('instance')
  );
}

test('ACC-049, ADM-002: a permission change reaches an open tab', async ({ page }, testInfo) => {
  const messages = collectMessages(page);
  await signIn(page);
  await expect.poll(() => messages.some(subscribedToInstance)).toBe(true);

  // Creating a project gives its creator a role there: everyone's access is checked again.
  const reload = page.waitForRequest(
    (request) => request.method() === 'GET' && request.url().endsWith('/api/v1/users/me/access'),
  );
  const suffix = `${testInfo.project.name === 'phone' ? 'P' : 'D'}${String(Date.now()).slice(-6)}`;
  const created = await page.request.post('/api/v1/projects', {
    data: { name: `Live ${suffix}`, key: `L${suffix}`, template: 'blank' },
  });
  expect(created.status()).toBe(201);
  await expect
    .poll(() =>
      messages.some(
        (message) =>
          message.type === 'event' &&
          (message.entity as { kind?: unknown } | undefined)?.kind === 'access',
      ),
    )
    .toBe(true);
  // Within the random spread of 0–2 s.
  await reload;
});

test('ADM-002: after a reconnect the tab loads the settings again', async ({ page }) => {
  let connections = 0;
  let closedFirst = false;
  let reloadedAfter = false;
  page.on('request', (request) => {
    if (closedFirst && request.method() === 'GET' && request.url().endsWith('/api/v1/bootstrap'))
      reloadedAfter = true;
  });
  await page.routeWebSocket(`**${LIVE}`, (socket) => {
    connections += 1;
    const first = connections === 1;
    const server = socket.connectToServer();
    server.onMessage((message) => {
      socket.send(message);
      if (!first || typeof message !== 'string') return;
      if (!subscribedToInstance(JSON.parse(message) as Message)) return;
      // The node restarts: the first socket is closed as the server would close it.
      closedFirst = true;
      void socket.close({ code: 1012, reason: 'restart' });
    });
  });
  await signIn(page);

  // Notices are not replayed, so the second `subscribed` makes the tab load the settings again.
  await expect.poll(() => reloadedAfter).toBe(true);
  expect(connections).toBe(2);
});
