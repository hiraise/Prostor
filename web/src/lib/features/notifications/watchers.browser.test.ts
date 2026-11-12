// The watcher popup of the issue page (NTF-015, NTF-016; plan step 1.10 §4.3): the counter next
// to the star opens the list, those with `issue.watchers.update` add people who can read the
// issue and take anyone off, everyone may take their own marker off, and without
// `issue.watchers.read` there is no counter at all. Component tests run on a desktop and on a
// phone (D-019).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { Issue, UserRef } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible } from '../../../test/a11y.ts';
import { fakeApi } from '../../../test/fake-api.ts';
import { issue, ANNA, BORIS } from '../issues/test-data.ts';
import WatchersPopup from './WatchersPopup.svelte';

const VERA: UserRef = {
  id: 3,
  username: 'vera',
  full_name: 'Vera Person',
  avatar_color: 0,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

function shown(rest: Partial<Issue> = {}): Issue {
  return issue({ watchers: 2, can: { ...issue().can, manage_watchers: true }, ...rest });
}

/** Renders the popup with the session of Anna (id 1) and the issue in the store. */
async function show(viewed: Issue) {
  setSessionForTests({
    user: {
      id: 1,
      username: 'anna',
      full_name: 'Anna Admin',
      email: null,
      email_verified: false,
      avatar_color: 0,
      avatar_url: null,
      version: 1,
    },
    remember: false,
    created_at: '2026-10-22T09:00:00Z',
    password_change_required: false,
  });
  store.entities.table('issue').upsert(viewed, 'full');
  return render(WatchersPopup, { issue: viewed });
}

afterEach(() => {
  vi.unstubAllGlobals();
  store.entities.table('issue').clear();
  setSessionForTests(null);
  setLocale(BASE_LOCALE);
});

describe('WatchersPopup', () => {
  it('NTF-015, NTF-016: opens the list, removes and adds watchers', async () => {
    // The list the server holds; every PUT replaces it, as the GET answers it.
    let held: UserRef[] = [ANNA, BORIS];
    const calls = fakeApi({
      'GET /issues/501/watchers': () => ({ body: { users: held } }),
      'PUT /issues/501/watchers': (request) => {
        held = (request.body as { users: number[] }).users.map(
          (id) => [ANNA, BORIS, VERA].find((user) => user.id === id) ?? ANNA,
        );
        return { body: { users: held } };
      },
      'GET /issues/501/mention-candidates': () => ({ body: { users: [ANNA, BORIS, VERA] } }),
    });
    const screen = await show(shown());

    await screen.getByRole('button', { name: '2 watchers' }).click();
    await expect.element(screen.getByText('Anna Admin')).toBeVisible();
    await expect.element(screen.getByText('Boris Lead')).toBeVisible();

    // Removing Boris replaces the whole list without him.
    await screen.getByRole('button', { name: 'Stop Boris Lead watching' }).click();
    expect(calls.at(-1)?.body).toEqual({ users: [1] });
    // Boris left the list, but stays among the people to add.
    expect(screen.getByRole('button', { name: 'Stop Boris Lead watching' }).query()).toBeNull();

    // Adding offers only people who do not watch yet; picking Vera adds her.
    await screen.getByRole('radio', { name: /Vera Person/ }).click();
    expect(calls.at(-1)?.body).toEqual({ users: [1, 3] });
    await expect.element(screen.getByText('Vera Person')).toBeVisible();

    expect(screen.getByRole('radio', { name: /Anna Admin/ }).query()).toBeNull();
    await expectAccessible(screen.container);
  });

  it('NTF-016: without the right only one’s own marker moves', async () => {
    fakeApi({
      'GET /issues/501/watchers': () => ({ body: { users: [ANNA, BORIS] } }),
    });
    const screen = await show(shown({ can: { ...issue().can, manage_watchers: false } }));

    await screen.getByRole('button', { name: '2 watchers' }).click();
    await expect.element(screen.getByText('Boris Lead')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Stop Boris Lead watching' }).query()).toBeNull();
    expect(screen.getByRole('group', { name: 'Add a watcher' }).query()).toBeNull();
    await expect
      .element(screen.getByRole('button', { name: 'Stop Anna Admin watching' }))
      .toBeVisible();
  });

  it('NTF-015: no counter without `issue.watchers.read`', async () => {
    const screen = await show(shown({ watchers: null }));
    expect(screen.getByRole('button', { name: /watchers/ }).query()).toBeNull();
  });
});
