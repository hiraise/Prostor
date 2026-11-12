// Live updates of the open issue on a desktop and on a phone (step 1.7b-6; ISS-048): the page
// follows `issue:<id>` through a fake connection the test drives by hand.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, ActivityItem, Issue, LiveEvent } from '#lib/api/schema.gen.ts';
import type { TopicHandlers } from '#lib/live/connection.ts';
import { setAccessForTests, setPreferencesForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible } from '../../../../test/a11y.ts';
import {
  fakeApi,
  type FakeAnswer,
  type FakeRequest,
  type FakeRoute,
} from '../../../../test/fake-api.ts';
import IssuePageHarness from '../../../../test/harness/IssuePageHarness.svelte';
import { forgetIssuePages, type IssuePageData } from '../issues.ts';
import { comment, commentItem, issue } from '../test-data.ts';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

const ID = 501;
const TOPIC = `issue:${String(ID)}`;

const MEMBER: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic', 'issue.update', 'comment.create'] }],
  capabilities: [],
};

/** A connection whose topics the test drives by hand. */
class FakeConnection {
  readonly handlers = new Map<string, TopicHandlers>();

  subscribe(topic: string, handlers: TopicHandlers): () => void {
    this.handlers.set(topic, handlers);
    return () => {
      this.handlers.delete(topic);
    };
  }

  get issue(): TopicHandlers {
    const handlers = this.handlers.get(TOPIC);
    if (!handlers) throw new Error(`not subscribed to ${TOPIC}`);
    return handlers;
  }

  send(kind: string, id: number, version: number, change: LiveEvent['change']): void {
    this.issue.event?.({
      id: 1,
      topics: [TOPIC],
      entity: { kind, id, version },
      change,
      patch: null,
    });
  }
}

/** Stands in for the browser's observer, so a test says whether the stream's end is on screen. */
class FakeObserver {
  static all: FakeObserver[] = [];
  readonly callback: (entries: { isIntersecting: boolean }[]) => void;

  constructor(callback: (entries: { isIntersecting: boolean }[]) => void) {
    this.callback = callback;
    FakeObserver.all.push(this);
  }

  static show(visible: boolean): void {
    for (const observer of FakeObserver.all) observer.callback([{ isIntersecting: visible }]);
  }

  observe(): void {
    // The test decides.
  }

  disconnect(): void {
    FakeObserver.all = FakeObserver.all.filter((other) => other !== this);
  }
}

const FIRST_COMMENT = comment(31, 'First look');
const FIRST = commentItem(90, FIRST_COMMENT);

async function show(routes: Record<string, FakeRoute> = {}, shown: Issue = issue({ fields: [] })) {
  vi.stubGlobal('IntersectionObserver', FakeObserver);
  const calls = fakeApi(routes);
  store.entities.table('issue').upsert(shown, 'full');
  store.put('comment', [FIRST_COMMENT], 'full');
  const connection = new FakeConnection();
  const data: IssuePageData = {
    activity: { items: [FIRST], older: null, newer: null },
    links: { groups: [], can_link: false },
    attachments: { attachments: [], next: null },
  };
  const screen = await render(IssuePageHarness, { id: ID, page: data, connection });
  await expect.element(screen.getByText('First look')).toBeVisible();
  return { screen, calls, connection };
}

function newer(items: ActivityItem[]): FakeAnswer {
  return { body: { items, older: null, newer: null } };
}

function asked(calls: FakeRequest[], path: string): FakeRequest[] {
  return calls.filter((call) => call.path === path);
}

beforeEach(() => {
  setAccessForTests(MEMBER);
  setPreferencesForTests({ activity_order: 'oldest_first' });
});

afterEach(() => {
  vi.unstubAllGlobals();
  FakeObserver.all = [];
  store.entities.table('issue').clear();
  store.entities.table('comment').clear();
  store.entities.table('attachment').clear();
  forgetIssuePages();
  setAccessForTests(null);
  setPreferencesForTests(null);
});

describe('ISS-048: live updates of the open issue', () => {
  it('a field changed elsewhere shows at once, and the stream asks for what is newer', async () => {
    const { screen, calls, connection } = await show({
      'GET /issues/501': { body: issue({ summary: 'Login button fixed', fields: [], version: 5 }) },
      'GET /issues/501/activity': newer([]),
    });

    connection.send('issue', ID, 5, 'updated');
    await expect
      .element(screen.getByRole('heading', { level: 1, name: 'Login button fixed' }))
      .toBeVisible();
    await expect.poll(() => asked(calls, '/issues/501/activity')).toHaveLength(1);
    const query = asked(calls, '/issues/501/activity')[0]?.query;
    expect([query?.get('from'), query?.get('direction')]).toEqual(['90', 'newer']);
  });

  it('a new comment is appended at the end while the end is on screen', async () => {
    const { screen, connection } = await show({
      'GET /issues/501/activity': newer([commentItem(91, comment(32, 'Fixed on staging'))]),
    });
    FakeObserver.show(true);

    connection.send('comment', 32, 1, 'created');
    await expect.element(screen.getByText('Fixed on staging')).toBeVisible();
    expect(screen.getByText(/^New entries/).query()).toBeNull();
    const texts = screen
      .getByRole('listitem')
      .elements()
      .map((item) => item.textContent);
    expect(texts.findIndex((text) => text.includes('Fixed on staging'))).toBeGreaterThan(
      texts.findIndex((text) => text.includes('First look')),
    );
  });

  it('scrolled away, the stream says how many entries came and shows them on «Show»', async () => {
    let answer = 0;
    const { screen, calls, connection } = await show({
      'GET /issues/501/activity': () => {
        answer += 1;
        return newer(
          answer === 1
            ? [commentItem(91, comment(32, 'Fixed on staging'))]
            : [commentItem(92, comment(33, 'Deployed'))],
        );
      },
    });
    FakeObserver.show(false);

    connection.send('comment', 32, 1, 'created');
    await expect.element(screen.getByText(/^New entries: 1/)).toBeVisible();
    expect(screen.getByText('Fixed on staging').query()).toBeNull();
    connection.send('comment', 33, 1, 'created');
    await expect.element(screen.getByText(/^New entries: 2/)).toBeVisible();
    // The second fetch starts after the waiting item.
    expect(asked(calls, '/issues/501/activity')[1]?.query.get('from')).toBe('91');
    await expectAccessible(screen.container);

    await screen.getByRole('button', { name: 'Show', exact: true }).click();
    await expect.element(screen.getByText('Fixed on staging')).toBeVisible();
    await expect.element(screen.getByText('Deployed')).toBeVisible();
    expect(screen.getByText(/^New entries/).query()).toBeNull();
  });

  it('a deleted issue leaves the page', async () => {
    const { screen, connection } = await show();
    connection.send('issue', ID, 5, 'deleted');
    await expect.element(screen.getByText('Gone')).toBeVisible();
  });

  it('an issue no longer visible leaves the page when its topic is refused', async () => {
    const { screen, connection } = await show();
    connection.issue.denied?.();
    await expect.element(screen.getByText('Gone')).toBeVisible();
  });

  it('in edit mode a newer text warns and offers the differences', async () => {
    const { screen, connection } = await show({
      'GET /issues/501': {
        body: issue({ summary: 'Login button is broken', fields: [], version: 5, text_version: 3 }),
      },
      'GET /issues/501/activity': newer([]),
    });
    await screen.getByRole('button', { name: 'Edit', exact: true }).click();

    connection.send('issue', ID, 5, 'updated');
    await expect
      .element(screen.getByText(/^The issue was changed while you were editing it/))
      .toBeVisible();
    await screen.getByRole('button', { name: 'Show differences' }).click();
    const dialog = page.getByRole('dialog', { name: 'Someone changed this issue' });
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog.getByRole('deletion')).toBeInTheDocument();
  });

  it('missed changes that cannot be replayed show a hint; «Refresh» loads the page again', async () => {
    const { screen, calls, connection } = await show({
      'GET /issues/501': { body: issue({ fields: [], version: 4 }) },
      'GET /issues/501/activity': newer([FIRST]),
      'GET /issues/501/links': { body: { groups: [], can_link: false } },
      'GET /issues/501/attachments': { body: { attachments: [], next: null } },
    });
    // A replayed reconnect says nothing.
    connection.issue.subscribed?.();
    expect(screen.getByText(/may be out of date/).query()).toBeNull();

    connection.issue.resync?.();
    await expect.element(screen.getByText(/may be out of date/)).toBeVisible();
    await screen.getByRole('button', { name: 'Refresh' }).click();
    await expect.element(screen.getByText(/may be out of date/)).not.toBeInTheDocument();
    expect(asked(calls, '/issues/501/links')).toHaveLength(1);
    expect(asked(calls, '/issues/501/attachments')).toHaveLength(1);
  });
});
