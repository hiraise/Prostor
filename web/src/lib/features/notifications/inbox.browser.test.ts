// The bell, the badge and the inbox popup (NTF-047–NTF-050, NTF-053; plan step 1.10 §4.1):
// folders, threads with their rows, the read state everywhere, paging, and the live follow of
// the `inbox:` topic. Component tests run on a desktop and on a phone (D-019).
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  InboxNotification,
  InboxSummary,
  InboxThread,
  Session,
  UserRef,
} from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import type { TopicHandlers } from '#lib/live/connection.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';

import AppShell from '../../shell/AppShell.svelte';
import { expectAccessible, isPhone } from '../../../test/a11y.ts';
import { fakeApi, type FakeRequest } from '../../../test/fake-api.ts';
import {
  followInbox,
  inboxSummary,
  setInboxSummaryForTests,
  type InboxSource,
} from './inbox.svelte.ts';
import InboxPopup from './InboxPopup.svelte';
import NotificationBell from './NotificationBell.svelte';

const AUTHOR: UserRef = {
  id: 2,
  username: 'ada',
  full_name: 'Ada Lovelace',
  avatar_color: 1,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

const CHANGE = {
  issue: { readable_id: 'WEB-12', summary: 'Move the button' },
  lines: [{ t: 'text' as const, text: 'tags: +urgent' }],
  excerpt: 'WEB-12: tags: +urgent',
};

const SESSION: Session = {
  user: {
    id: 7,
    username: 'anna',
    full_name: 'Anna Admin',
    email: 'anna@example.com',
    email_verified: false,
    avatar_color: 3,
    avatar_url: null,
    version: 4,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

function row(id: number, over: Partial<InboxNotification> = {}): InboxNotification {
  return {
    id,
    issue_id: 1,
    kind: 'updated',
    author: AUTHOR,
    content: CHANGE,
    created_at: '2026-11-07T10:00:00Z',
    last: false,
    read: false,
    reasons: ['watch'],
    ...over,
  };
}

function reportRow(id: number): InboxNotification {
  return {
    ...row(id),
    issue_id: null,
    kind: 'system',
    reasons: ['system'],
    content: { job: 'Import', state: 'finished', message: 'Done' },
  };
}

function thread(rows: InboxNotification[]): InboxThread {
  const head = rows[0];
  if (!head) throw new Error('a thread needs at least one row');
  return { head, latest: rows };
}

/** A page of two threads: one issue thread and one standing-alone `system` row. */
function pageOf(
  rows: InboxNotification[],
  next: number | null,
): { threads: InboxThread[]; next: number | null } {
  return { threads: [thread(rows), { head: reportRow(90), latest: [] }], next };
}

const SUMMARY: InboxSummary = { total: 2, direct: 1, subscriptions: 1, system: 0 };

/** A live connection that only records subscriptions; `emit` feeds every handler an inbox event. */
function fakeConnection(): {
  connection: InboxSource;
  topics: string[];
  emit(patch: unknown): void;
} {
  const topics: string[] = [];
  const handlers: TopicHandlers[] = [];
  return {
    connection: {
      subscribe: (topic, topicHandlers) => {
        topics.push(topic);
        handlers.push(topicHandlers);
        return () => undefined;
      },
    },
    topics,
    emit(patch: Record<string, unknown>): void {
      const event = {
        type: 'event' as const,
        id: 1,
        change: 'created' as const,
        entity: { id: 7, kind: 'inbox' as const, version: 1 },
        patch,
        topics: [] as string[],
      };
      for (const handler of handlers) handler.event?.(event);
    },
  };
}

/** The calls the fake API saw for one method and path. */
function calls(requests: FakeRequest[], method: string, path: string): FakeRequest[] {
  return requests.filter((request) => request.method === method && request.path === path);
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  setInboxSummaryForTests(null);
  setLocale(BASE_LOCALE);
});

describe('NotificationBell', () => {
  it('NTF-048: the dot and the label follow the summary and the inbox topic', async () => {
    const connection = fakeConnection();
    const requests = fakeApi({ 'GET /notifications/summary': { body: SUMMARY } });
    const screen = await render(NotificationBell, {
      user: SESSION.user,
      connection: connection.connection,
    });

    await expect
      .element(screen.getByRole('button', { name: 'Notifications: 2 unread items' }))
      .toBeVisible();
    expect(screen.container.querySelector('.dot')).not.toBeNull();
    expect(connection.topics).toEqual(['inbox:7']);

    // A row arrived: every folder count grows by its membership.
    connection.emit({
      kind: 'notification',
      id: 9,
      event: 'updated',
      issue: 'WEB-12',
      author: 'Ada',
      excerpt: 'WEB-12: changed',
      reasons: ['mention'],
    });
    await expect
      .element(screen.getByRole('button', { name: 'Notifications: 3 unread items' }))
      .toBeVisible();

    // The read state changed elsewhere: the counts are set, not added.
    connection.emit({ kind: 'read', total: 1, direct: 0, subscriptions: 1, system: 0 });
    await expect
      .element(screen.getByRole('button', { name: 'Notifications: 1 unread item' }))
      .toBeVisible();

    connection.emit({ kind: 'read', total: 0, direct: 0, subscriptions: 0, system: 0 });
    await expect.element(screen.getByRole('button', { name: 'Notifications' })).toBeVisible();
    expect(screen.container.querySelector('.dot')).toBeNull();
    // The badge never refetches on events: the one load of the follow is all.
    expect(calls(requests, 'GET', '/notifications/summary')).toHaveLength(1);
  });

  it('UI-001: the bell sits in the shell, in the rail on wide screens and the top bar on phones', async () => {
    fakeApi({
      'GET /notifications/summary': { body: { total: 0, direct: 0, subscriptions: 0, system: 0 } },
    });
    setSessionForTests(SESSION);
    const content = createRawSnippet(() => ({ render: () => '<h1>Page title</h1>' }));
    const screen = await render(AppShell, { pathname: '/issues', children: content });

    await expect.element(screen.getByRole('button', { name: 'Notifications' })).toBeVisible();
    // Phones reach it without opening the drawer; wide screens have it in the rail.
    if (isPhone()) {
      expect(screen.container.querySelector('.topbar .bell')).not.toBeNull();
    } else {
      expect(screen.container.querySelector('.rail-footer .bell')).not.toBeNull();
    }
  });
});

describe('InboxPopup', () => {
  it('NTF-049: folder tabs, threads with their rows, and the folders the query asks for', async () => {
    const requests = fakeApi({
      'GET /notifications': () => ({
        body: pageOf(
          [row(11, { read: true }), row(12, { read: true }), row(13, { read: true })],
          null,
        ),
      }),
    });
    const screen = await render(InboxPopup);

    // Three folders for everyone; «Система» joins for people who see the administration area.
    for (const name of ['All', 'Mentions', 'Subscriptions']) {
      await expect.element(screen.getByRole('button', { name })).toBeVisible();
    }
    await expect.element(screen.getByRole('button', { name: 'System' })).not.toBeInTheDocument();
    await expect
      .element(screen.getByRole('button', { name: 'All' }))
      .toHaveAttribute('aria-pressed', 'true');

    // The thread shows its rows inline; a `system` row reports its job.
    await expect.element(screen.getByText('WEB-12').first()).toBeVisible();
    expect(screen.container.querySelectorAll('.row')).toHaveLength(4);
    await expect.element(screen.getByText('Import — finished: Done')).toBeVisible();

    await userEvent.click(screen.getByRole('button', { name: 'Mentions' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'GET', '/notifications').at(-1)?.query.get('folder')).toBe('direct');
    });

    await userEvent.click(screen.getByRole('checkbox', { name: 'Unread only' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'GET', '/notifications').at(-1)?.query.get('unread')).toBe('only');
    });
  });

  it('NTF-022: the texts of the folders and kinds exist in Russian too', async () => {
    setLocale('ru');
    fakeApi({
      'GET /notifications': {
        body: { threads: [thread([row(11, { kind: 'resolved' })])], next: null },
      },
    });
    const screen = await render(InboxPopup);

    for (const name of ['Все', 'Упоминания', 'Подписки']) {
      await expect.element(screen.getByRole('button', { name })).toBeVisible();
    }
    await expect.element(screen.getByText('решена')).toBeVisible();
  });

  it('NTF-049: «Показать ещё» pages the list with the cursor the answer named', async () => {
    const requests = fakeApi({
      'GET /notifications': (request) => {
        if (request.query.get('before') === '100') {
          return {
            body: { threads: [thread([row(21, { issue_id: 2, read: true })])], next: null },
          };
        }
        return { body: pageOf([row(11, { read: true }), row(12, { read: true })], 100) };
      },
    });
    const screen = await render(InboxPopup);
    await expect.element(screen.getByText('WEB-12').first()).toBeVisible();

    // Two inline rows hide the thread's own «Показать ещё»; the one shown is the list's.
    await userEvent.click(screen.getByRole('button', { name: 'Show more' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'GET', '/notifications').at(-1)?.query.get('before')).toBe('100');
    });
    // The second page's thread joins the list once.
    expect(screen.container.querySelectorAll('.thread')).toHaveLength(3);
  });

  it('NTF-049: «Показать ещё» of a thread loads its whole self', async () => {
    const requests = fakeApi({
      'GET /notifications': (request) => {
        if (request.query.get('issue') === '1') {
          const rows = [21, 22, 23, 24, 25].map((id) => row(id, { read: true }));
          return { body: { threads: rows.map((one) => thread([one])), next: null } };
        }
        return {
          body: pageOf(
            [row(11, { read: true }), row(12, { read: true }), row(13, { read: true })],
            null,
          ),
        };
      },
    });
    const screen = await render(InboxPopup);
    await expect.element(screen.getByText('Import — finished: Done')).toBeVisible();
    expect(screen.container.querySelectorAll('.row')).toHaveLength(4);

    await userEvent.click(screen.getByRole('button', { name: 'Show more' }));
    await vi.waitFor(() => {
      // The whole thread replaces the three inline rows.
      expect(screen.container.querySelectorAll('.row')).toHaveLength(6);
    });
    expect(calls(requests, 'GET', '/notifications').at(-1)?.query.get('issue')).toBe('1');
  });

  it('NTF-050: rows, threads and everything turn read and unread again', async () => {
    const requests = fakeApi({
      'GET /notifications': () => ({ body: pageOf([row(11), row(12, { read: true })], null) }),
      'POST /notifications/read': { body: { total: 0, direct: 0, subscriptions: 0, system: 0 } },
    });
    const screen = await render(InboxPopup);
    // The rows of the issue thread, apart from the standing-alone `system` row.
    const issueThread = screen.getByRole('region', { name: 'WEB-12: tags: +urgent' });

    await userEvent.click(issueThread.getByRole('button', { name: 'Mark as read' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'POST', '/notifications/read').at(-1)?.body).toEqual({
        ids: [11],
        unread: false,
      });
    });
    // The read answer sets the badge's counts.
    expect(inboxSummary()).toEqual({ total: 0, direct: 0, subscriptions: 0, system: 0 });

    await userEvent.click(issueThread.getByRole('button', { name: 'Mark as unread' }).nth(1));
    await vi.waitFor(() => {
      expect(calls(requests, 'POST', '/notifications/read').at(-1)?.body).toEqual({
        ids: [12],
        unread: true,
      });
    });

    await userEvent.click(screen.getByRole('button', { name: 'Mark thread as read' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'POST', '/notifications/read').at(-1)?.body).toEqual({ issue: 1 });
    });

    await userEvent.click(screen.getByRole('button', { name: 'Mark all as read' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'POST', '/notifications/read').at(-1)?.body).toEqual({ all: true });
    });
  });

  it('plan §4.1: a row that arrives over the topic reloads the open popup', async () => {
    const requests = fakeApi({
      'GET /notifications': { body: { threads: [thread([row(11)])], next: null } },
      'GET /notifications/summary': { body: SUMMARY },
    });
    const connection = fakeConnection();
    const stop = followInbox(7, { connection: connection.connection });
    const screen = await render(InboxPopup);
    await expect.element(screen.getByText('WEB-12').first()).toBeVisible();
    await vi.waitFor(() => {
      expect(calls(requests, 'GET', '/notifications')).toHaveLength(1);
    });

    connection.emit({
      kind: 'notification',
      id: 21,
      event: 'comment',
      issue: 'WEB-12',
      author: 'Ada',
      excerpt: 'WEB-12: a comment',
      reasons: ['watch'],
    });
    await vi.waitFor(() => {
      expect(calls(requests, 'GET', '/notifications')).toHaveLength(2);
    });
    stop();
  });

  it('holds in both profiles (D-019) and stays accessible (UI-032)', async () => {
    fakeApi({
      'GET /notifications': () => ({ body: pageOf([row(11), row(12, { read: true })], null) }),
    });
    const screen = await render(InboxPopup);
    await expect.element(screen.getByText('WEB-12').first()).toBeVisible();
    await expectAccessible(document.body);
  });
});
