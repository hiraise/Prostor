// One's picture on the account page (ACC-017): upload, Gravatar, removal and problems, on a
// desktop and on a phone.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { Account, Bootstrap, Session, Upload } from '#lib/api/schema.gen.ts';
import { setBootstrapForTests } from '#lib/api/system.ts';
import { keepAccount } from '#lib/features/access/index.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { getSession, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import { failure, fakeApi } from '../../../test/fake-api.ts';
import { xhrThroughFetch } from '../../../test/fake-xhr.ts';
import { PICTURE } from '../../../test/pictures.ts';
import AvatarForm from './AvatarForm.svelte';

const ACCOUNT: Account = {
  id: 1,
  username: 'anna',
  full_name: 'Anna Admin',
  avatar_color: 3,
  avatar_url: null,
  email: 'anna@example.com',
  email_verified: true,
  guest: false,
  invited: false,
  disabled: null,
  groups: [],
  projects: [],
  full_profile: true,
  version: 4,
  created_at: '2026-10-01T09:00:00Z',
  tier: 'full',
  password: { has_password: true, must_change: false },
};

const SESSION: Session = {
  user: {
    id: 1,
    username: 'anna',
    full_name: 'Anna Admin',
    email: 'anna@example.com',
    email_verified: true,
    avatar_color: 3,
    avatar_url: null,
    version: 4,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

const BOOTSTRAP: Bootstrap = {
  version: '0.1.0',
  base_url: null,
  setup_required: false,
  email_enabled: true,
  gravatar: true,
  default_date_format: 'locale',
  default_home: 'issues',
  work_schedule: { day_minutes: 480, week_days: 5 },
  limits: { upload_max_bytes: 1024 },
};

const UPLOAD: Upload = {
  id: 31,
  name: 'me.png',
  size: 79,
  content_type: 'image/png',
  kind: 'image',
  url: '/api/v1/uploads/31/me.png',
  created_at: '2026-10-22T09:00:00Z',
};

/** The stored account row, as the account page passes it. */
function account(extra: Partial<Account> = {}): Account {
  keepAccount({ ...ACCOUNT, ...extra, version: ACCOUNT.version + ++bump }, 'full');
  const row = store.entities.table('account').get(ACCOUNT.id);
  if (!row || !('tier' in row)) throw new Error('No account row');
  return row;
}
let bump = 0;

function png(size = 79): File {
  return new File([new Uint8Array(size)], 'me.png', { type: 'image/png' });
}

function fileInput(container: Element): HTMLInputElement {
  const input = container.querySelector('input[type="file"]');
  if (!(input instanceof HTMLInputElement)) throw new Error('No file input');
  return input;
}

beforeEach(() => {
  setSessionForTests(SESSION);
  setBootstrapForTests(BOOTSTRAP);
  xhrThroughFetch();
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  setSessionForTests(null);
  setBootstrapForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('AvatarForm', () => {
  it('ACC-017: uploads a photo and shows it here and in the session', async () => {
    const calls = fakeApi({
      'POST /uploads': { status: 201, body: UPLOAD },
      'PUT /users/1/avatar': { body: { url: PICTURE } },
    });
    const screen = await render(AvatarForm, { account: account(), gravatar: true });
    await expect.element(screen.getByText('Anna Admin')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Remove photo' }).query()).toBeNull();

    const opened = vi
      .spyOn(HTMLInputElement.prototype, 'click')
      .mockImplementation(() => undefined);
    await screen.getByRole('button', { name: 'Upload photo' }).click();
    expect(opened).toHaveBeenCalledOnce();
    expect(fileInput(screen.container).accept).toContain('image/png');
    await userEvent.upload(fileInput(screen.container), png());

    await vi.waitFor(() => {
      expect(screen.container.querySelector('img')?.getAttribute('src')).toBe(PICTURE);
    });
    const upload = calls.find((call) => call.path === '/uploads');
    expect(upload?.query.get('name')).toBe('me.png');
    expect(upload?.body).toBeInstanceOf(Blob);
    expect(calls.find((call) => call.method === 'PUT')?.body).toEqual({ upload: 31 });
    expect(getSession()?.user.avatar_url).toBe(PICTURE);
    await expect.element(screen.getByRole('button', { name: 'Remove photo' })).toBeVisible();
    expect(toasts.items.at(-1)?.message).toBe('Photo updated.');
    expect(smallTargets(screen.container, isPhone() ? 44 : 28)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ACC-017: takes the picture from Gravatar and says when there is none', async () => {
    let answers = 0;
    fakeApi({
      'POST /users/1/avatar/gravatar': () =>
        ++answers === 1 ? failure(404, 'gravatar_not_found') : { body: { url: PICTURE } },
    });
    const screen = await render(AvatarForm, { account: account(), gravatar: true });

    await screen.getByRole('button', { name: 'Use Gravatar' }).click();
    await expect
      .element(screen.getByText('Gravatar has no picture for your email address.'))
      .toBeVisible();

    await screen.getByRole('button', { name: 'Use Gravatar' }).click();
    await vi.waitFor(() => {
      expect(screen.container.querySelector('img')?.getAttribute('src')).toBe(PICTURE);
    });
    expect(screen.getByText('Gravatar has no picture for your email address.').query()).toBeNull();
    expect(getSession()?.user.avatar_url).toBe(PICTURE);
  });

  it('ACC-017: offers no Gravatar when the server may not ask it', async () => {
    fakeApi({});
    const screen = await render(AvatarForm, { account: account(), gravatar: false });
    await expect.element(screen.getByRole('button', { name: 'Upload photo' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Use Gravatar' }).query()).toBeNull();
  });

  it('ACC-017: removes the photo, and the initials come back', async () => {
    const calls = fakeApi({ 'DELETE /users/1/avatar': { status: 204 } });
    setSessionForTests({ ...SESSION, user: { ...SESSION.user, avatar_url: PICTURE } });
    const screen = await render(AvatarForm, {
      account: account({ avatar_url: PICTURE }),
      gravatar: false,
    });

    await screen.getByRole('button', { name: 'Remove photo' }).click();
    await vi.waitFor(() => {
      expect(screen.container.querySelector('img')).toBeNull();
    });
    expect(calls.map((call) => `${call.method} ${call.path}`)).toEqual(['DELETE /users/1/avatar']);
    expect(getSession()?.user.avatar_url).toBeNull();
    expect(screen.getByRole('button', { name: 'Remove photo' }).query()).toBeNull();
    expect(toasts.items.at(-1)?.message).toBe('Photo removed. Your initials are shown instead.');
  });

  it('ACC-017: a file over the limit is refused before sending, in Russian too', async () => {
    setLocale('ru');
    const calls = fakeApi({});
    const screen = await render(AvatarForm, { account: account(), gravatar: true });
    await userEvent.upload(fileInput(screen.container), png(2048));

    await expect.element(screen.getByText('Файл или текст слишком большой.')).toBeVisible();
    expect(calls).toEqual([]);
    await expect.element(screen.getByRole('button', { name: 'Загрузить фото' })).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Взять из Gravatar' })).toBeVisible();
  });

  it('ACC-017: a file that is not a picture is explained', async () => {
    fakeApi({
      'POST /uploads': { status: 201, body: { ...UPLOAD, kind: 'other' } },
      'PUT /users/1/avatar': failure(400, 'invalid_request', [{ path: 'upload', code: 'invalid' }]),
    });
    const screen = await render(AvatarForm, { account: account(), gravatar: false });
    await userEvent.upload(fileInput(screen.container), png());
    await expect
      .element(
        screen.getByText('This file is not a picture. Choose a PNG, JPEG, GIF or WebP file.'),
      )
      .toBeVisible();
  });
});
