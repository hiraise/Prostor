// The «Уведомления» tab of the account page and subscriptions (NTF-017, NTF-021–NTF-025,
// NTF-093, NTF-094; plan step 1.10 §4.2): the settings that apply at once, the address with its
// test letter, the subscription list with create (the type-ahead), change and remove, and the
// subscription sections of the tag and saved-search dialogs. Component tests run on a desktop
// and on a phone (D-019).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type {
  NotificationSettings,
  NotificationSubscription,
  SavedSearch,
  Tag,
  UserRef,
} from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';

import SearchDialog from '../issues/sidebar/SearchDialog.svelte';
import TagDialog from '../issues/sidebar/TagDialog.svelte';
import { expectAccessible } from '../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../test/fake-api.ts';
import SettingsForm from './NotificationSettings.svelte';
import SubscriptionList from './SubscriptionList.svelte';

const ANNA: UserRef = {
  id: 1,
  username: 'anna',
  full_name: 'Anna Admin',
  avatar_color: 0,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

const SETTINGS: NotificationSettings = {
  email: true,
  email_html: true,
  mentions: true,
  own_changes: false,
  watch_created: true,
  watch_updated: true,
  watch_resolved: true,
  watch_comment: true,
  watch_vote: true,
  watch_tag: true,
  autowatch_created: true,
  autowatch_commented: true,
  autowatch_assigned: true,
  autowatch_voted: true,
};

function sub(
  id: number,
  source: 'tag' | 'search',
  sourceId: number,
  name: string,
  kinds: NotificationSubscription['kinds'],
): NotificationSubscription {
  return { id, source, source_id: sourceId, name, kinds, created_at: '2026-11-07T09:00:00Z' };
}

const TAG: Tag = {
  id: 5,
  name: 'urgent',
  color: 3,
  owner: ANNA,
  sharing: {
    view: { users: [], groups: [] },
    use: { users: [], groups: [] },
    edit: { users: [], groups: [] },
  },
  abilities: { apply: true, edit: true, delete: true, share: true, transfer: false },
  version: 4,
  created_at: '2026-10-20T09:00:00Z',
};

const SEARCH: SavedSearch = {
  id: 7,
  name: 'My open',
  owner: ANNA,
  query: 'assignee: me',
  problems: [],
  builtin: null,
  project: null,
  sharing: { view: { users: [], groups: [] }, edit: { users: [], groups: [] } },
  abilities: { apply: true, edit: true, delete: true, share: true, transfer: false },
  version: 2,
  created_at: '2026-10-20T09:00:00Z',
  updated_at: '2026-10-20T09:00:00Z',
};

/** The calls the fake API saw for one method and path. */
function calls(requests: FakeRequest[], method: string, path: string): FakeRequest[] {
  return requests.filter((request) => request.method === method && request.path === path);
}

/** The routes every render of the settings half sees. */
function routes(extra: Record<string, FakeRoute> = {}): Record<string, FakeRoute> {
  return {
    'GET /users/me/notifications': { body: SETTINGS },
    'GET /notifications/subscriptions': { body: { subscriptions: [] } },
    'PUT /users/me/notifications': (request) => ({ body: request.body }),
    ...extra,
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
  setLocale(BASE_LOCALE);
});

describe('NotificationSettings', () => {
  it('NTF-017/NTF-030/NTF-034/NTF-066: every choice applies at once with the whole settings', async () => {
    const requests = fakeApi(routes({}));
    const screen = await render(SettingsForm, {
      email: 'anna@example.com',
      emailEnabled: true,
    });

    await expect
      .element(screen.getByRole('checkbox', { name: 'Send letters to the profile address' }))
      .toBeChecked();
    await expect.element(screen.getByRole('radio', { name: 'HTML' })).toBeChecked();

    // A channel off takes the format away and sends the whole settings.
    await userEvent.click(
      screen.getByRole('checkbox', { name: 'Send letters to the profile address' }),
    );
    await vi.waitFor(() => {
      expect(calls(requests, 'PUT', '/users/me/notifications').at(-1)?.body).toEqual({
        ...SETTINGS,
        email: false,
      });
    });
    await expect.element(screen.getByRole('radio', { name: 'HTML' })).not.toBeInTheDocument();

    // An event switch and a watch kind each send their own whole settings.
    await userEvent.click(screen.getByRole('checkbox', { name: 'My own changes' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'PUT', '/users/me/notifications').at(-1)?.body).toEqual({
        ...SETTINGS,
        email: false,
        own_changes: true,
      });
    });
    await userEvent.click(screen.getByRole('checkbox', { name: 'Votes' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'PUT', '/users/me/notifications').at(-1)?.body).toEqual({
        ...SETTINGS,
        email: false,
        own_changes: true,
        watch_vote: false,
      });
    });
  });

  it('a failed save returns the switch', async () => {
    const requests = fakeApi(routes({ 'PUT /users/me/notifications': failure(403, 'forbidden') }));
    const screen = await render(SettingsForm, {
      email: 'anna@example.com',
      emailEnabled: true,
    });

    await userEvent.click(screen.getByRole('checkbox', { name: 'Mentions' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'PUT', '/users/me/notifications')).toHaveLength(1);
    });
    await expect.element(screen.getByRole('checkbox', { name: 'Mentions' })).toBeChecked();
  });

  it('NTF-094: the address line, and the test letter when the instance sends email', async () => {
    const requests = fakeApi(
      routes({ 'POST /users/me/test-email': { body: { email: 'anna@example.com' } } }),
    );
    const screen = await render(SettingsForm, {
      email: 'anna@example.com',
      emailEnabled: true,
    });

    await expect.element(screen.getByText('Letters go to anna@example.com.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Send a test letter' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'POST', '/users/me/test-email')).toHaveLength(1);
    });
  });

  it('NTF-094: with instance email off, the notice stands in for the test letter', async () => {
    const requests = fakeApi(routes({}));
    const screen = await render(SettingsForm, {
      email: 'anna@example.com',
      emailEnabled: false,
    });

    await expect
      .element(screen.getByText('Instance email is turned off, so no letters are sent.'))
      .toBeVisible();
    expect(calls(requests, 'POST', '/users/me/test-email')).toHaveLength(0);
    await expect
      .element(screen.getByRole('button', { name: 'Send a test letter' }))
      .not.toBeInTheDocument();
  });

  it('NTF-022: the Russian texts of the settings exist', async () => {
    setLocale('ru');
    fakeApi(routes({}));
    const screen = await render(SettingsForm, {
      email: 'anna@example.com',
      emailEnabled: true,
    });

    for (const text of ['Письма', 'События', 'Наблюдение', 'Автонаблюдение', 'Подписки']) {
      await expect.element(screen.getByText(text).first()).toBeVisible();
    }
    await expect.element(screen.getByText('Письма уходят на anna@example.com.')).toBeVisible();
  });

  it('ISS-030: the auto-watch switches apply at once with the whole settings', async () => {
    const requests = fakeApi(routes({}));
    const screen = await render(SettingsForm, {
      email: 'anna@example.com',
      emailEnabled: true,
    });

    await expect
      .element(screen.getByRole('checkbox', { name: 'Issues assigned to me' }))
      .toBeChecked();
    await userEvent.click(screen.getByRole('checkbox', { name: 'Issues I publish' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'PUT', '/users/me/notifications').at(-1)?.body).toEqual({
        ...SETTINGS,
        autowatch_created: false,
      });
    });
  });

  it('holds in both profiles (D-019) and stays accessible (UI-032)', async () => {
    fakeApi(routes({}));
    const screen = await render(SettingsForm, {
      email: 'anna@example.com',
      emailEnabled: true,
    });
    await expect.element(screen.getByRole('checkbox', { name: 'Mentions' })).toBeVisible();
    await expectAccessible(document.body);
  });
});

describe('SubscriptionList', () => {
  const MINE = [
    sub(11, 'tag', 5, 'urgent', ['created', 'vote']),
    sub(12, 'search', 7, 'My open', []),
  ];

  function listRoutes(extra: Record<string, FakeRoute> = {}): Record<string, FakeRoute> {
    return {
      'GET /notifications/subscriptions': { body: { subscriptions: MINE } },
      ...extra,
    };
  }

  it('NTF-021/NTF-024: the rows say their source and what it delivers', async () => {
    fakeApi(listRoutes());
    const screen = await render(SubscriptionList);

    await expect.element(screen.getByText('urgent')).toBeVisible();
    await expect.element(screen.getByText('tag · Created, Votes')).toBeVisible();
    // An empty kind list keeps the subscription stored but delivering nothing (§3.1).
    await expect.element(screen.getByText('saved search · delivers nothing')).toBeVisible();
  });

  it('NTF-021: a subscription is created through the type-ahead and the kinds', async () => {
    const requests = fakeApi({
      'GET /notifications/subscriptions': {
        body: { subscriptions: [sub(11, 'tag', 5, 'urgent', ['created'])] },
      },
      'GET /tags': { body: { items: [TAG] } },
      'GET /saved-searches': { body: { items: [SEARCH] } },
      'POST /notifications/subscriptions': {
        status: 201,
        body: sub(13, 'search', 7, 'My open', ['created', 'updated', 'resolved', 'comment']),
      },
    });
    const screen = await render(SubscriptionList);

    await userEvent.click(screen.getByRole('button', { name: 'Subscribe' }).first());
    // Everything visible to me is offered, with what it is; an already-subscribed source — here
    // the tag — cannot be picked again.
    await expect.element(screen.getByRole('radio', { name: /urgent/ })).toBeVisible();
    await expect.element(screen.getByRole('radio', { name: /urgent/ })).toBeDisabled();

    // Save without a pick says what is missing first.
    await userEvent.click(screen.getByRole('button', { name: 'Subscribe' }).nth(1));
    await expect.element(screen.getByText('Choose what to subscribe to first.')).toBeVisible();

    await userEvent.click(screen.getByRole('radio', { name: /My open/ }));
    // A search offers none of the tag kinds (NTF-022).
    await expect
      .element(screen.getByRole('checkbox', { name: 'Tag added' }))
      .not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('checkbox', { name: 'Votes' }));
    await userEvent.click(screen.getByRole('button', { name: 'Subscribe' }).nth(1));
    await vi.waitFor(() => {
      expect(calls(requests, 'POST', '/notifications/subscriptions').at(-1)?.body).toEqual({
        source: 'search',
        source_id: 7,
        kinds: ['created', 'updated', 'resolved', 'comment'],
      });
    });
  });

  it('NTF-024: the kinds of a subscription change through «Изменить»', async () => {
    const requests = fakeApi(
      listRoutes({
        'PUT /notifications/subscriptions/11': { body: sub(11, 'tag', 5, 'urgent', ['created']) },
      }),
    );
    const screen = await render(SubscriptionList);

    await userEvent.click(
      screen.getByRole('button', { name: 'Change the subscription on urgent' }),
    );
    await expect.element(screen.getByRole('checkbox', { name: 'Votes' })).toBeChecked();
    await userEvent.click(screen.getByRole('checkbox', { name: 'Votes' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'PUT', '/notifications/subscriptions/11').at(-1)?.body).toEqual({
        kinds: ['created'],
      });
    });
  });

  it('NTF-023: a subscription is removed after a confirmation', async () => {
    const requests = fakeApi(listRoutes({}));
    const screen = await render(SubscriptionList);

    await userEvent.click(
      screen.getByRole('button', { name: 'Remove the subscription on urgent' }),
    );
    await expect.element(screen.getByText('Remove the subscription?')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'DELETE', '/notifications/subscriptions/11')).toHaveLength(1);
    });
  });
});

describe('SourceSubscription in the dialogs', () => {
  it('NTF-025: ticks in the tag dialog create and then empty my subscription', async () => {
    let created = false;
    const requests = fakeApi({
      'GET /notifications/subscriptions': () => ({
        body: { subscriptions: created ? [sub(21, 'tag', 5, 'urgent', ['created'])] : [] },
      }),
      'POST /notifications/subscriptions': () => {
        created = true;
        return { status: 201, body: sub(21, 'tag', 5, 'urgent', ['created']) };
      },
      'PUT /notifications/subscriptions/21': { body: sub(21, 'tag', 5, 'urgent', []) },
    });
    const screen = await render(TagDialog, { open: true, tag: TAG, onsaved: () => undefined });

    // A tag offers all seven kinds (NTF-022).
    await expect.element(screen.getByRole('checkbox', { name: 'Tag removed' })).toBeVisible();
    await userEvent.click(screen.getByRole('checkbox', { name: 'Created' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'POST', '/notifications/subscriptions').at(-1)?.body).toEqual({
        source: 'tag',
        source_id: 5,
        kinds: ['created'],
      });
    });

    // The last tick away keeps the subscription stored but silent (§3.1).
    await userEvent.click(screen.getByRole('checkbox', { name: 'Created' }));
    await vi.waitFor(() => {
      expect(calls(requests, 'PUT', '/notifications/subscriptions/21').at(-1)?.body).toEqual({
        kinds: [],
      });
    });
  });

  it('NTF-025/NTF-022: the search dialog offers none of the tag kinds', async () => {
    fakeApi({
      'GET /notifications/subscriptions': { body: { subscriptions: [] } },
    });
    const screen = await render(SearchDialog, {
      open: true,
      search: SEARCH,
      onsaved: () => undefined,
    });

    await expect.element(screen.getByRole('checkbox', { name: 'Created' })).toBeVisible();
    await expect
      .element(screen.getByRole('checkbox', { name: 'Tag added' }))
      .not.toBeInTheDocument();
  });
});
