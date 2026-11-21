// Signing in, the first-run setup, the password and API tokens, on a desktop and on a phone
// (step 0.5; ADM-047, AUTH-001, AUTH-003, AUTH-005, AUTH-009, AUTH-029, AUTH-030).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { ApiToken, Session } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setSessionForTests } from '#lib/session/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import { failure, fakeApi } from '../../../test/fake-api.ts';
import PasswordForm from './PasswordForm.svelte';
import SetupForm from './SetupForm.svelte';
import SignInForm from './SignInForm.svelte';
import TokenList from './TokenList.svelte';
import { ensureTokens } from './tokens.ts';

const SESSION: Session = {
  user: {
    id: 1,
    username: 'admin',
    full_name: 'Anna Admin',
    email: 'anna@example.com',
    email_verified: true,
    avatar_color: 0,
    avatar_url: null,
    version: 1,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

function token(id: number, name: string, scopes: ApiToken['scopes']): ApiToken {
  return {
    id,
    name,
    scopes,
    created_by: 1,
    created_at: '2026-10-20T09:00:00Z',
    last_used_at: null,
    version: 1,
  };
}

/** A field by its label; required fields add "*" to the label text. */
function label(text: string): RegExp {
  return new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\*?$`);
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setLocale(BASE_LOCALE);
});

describe('SignInForm', () => {
  it('AUTH-001: one message for a wrong username or password, then signs in', async () => {
    let attempts = 0;
    const calls = fakeApi({
      'POST /session': () =>
        ++attempts === 1
          ? failure(401, 'invalid_credentials')
          : { status: 201, body: { ...SESSION, remember: true } },
    });
    const onsignedin = vi.fn();
    const screen = await render(SignInForm, { onsignedin });

    await screen.getByLabelText(label('Username or email')).fill('admin');
    await screen.getByLabelText(label('Password')).fill('wrong horse');
    await screen.getByRole('button', { name: 'Sign in' }).click();
    await expect
      .element(screen.getByText('The username or password is not correct.'))
      .toBeVisible();
    await expect.element(screen.getByLabelText(label('Password'))).toHaveValue('');
    await expect.element(screen.getByLabelText(label('Password'))).toHaveFocus();
    expect(onsignedin).not.toHaveBeenCalled();

    await screen.getByLabelText(label('Password')).fill('correct horse');
    // AUTH-003: the whole row of "remember me" is the target.
    await screen.getByText('Remember me on this device').click();
    await screen.getByRole('button', { name: 'Sign in' }).click();
    await vi.waitFor(() => {
      expect(onsignedin).toHaveBeenCalledOnce();
    });
    expect(calls.at(-1)?.body).toEqual({
      login: 'admin',
      password: 'correct horse',
      remember: true,
    });
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('AUTH-001: asks for missing values without calling the server', async () => {
    const calls = fakeApi({});
    const screen = await render(SignInForm, { onsignedin: vi.fn() });
    await screen.getByRole('button', { name: 'Sign in' }).click();
    await expect.element(screen.getByText('Fill in this field.').first()).toBeVisible();
    expect(screen.getByText('Fill in this field.').elements()).toHaveLength(2);
    expect(calls).toEqual([]);
  });

  it('AUTH-005: says why the person is back, in Russian too', async () => {
    setLocale('ru');
    const screen = await render(SignInForm, { onsignedin: vi.fn(), reason: 'signed-out' });
    await expect.element(screen.getByText('Вы вышли из Простора.')).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Войти' })).toBeVisible();
    await expectAccessible(screen.container);
  });

  it('AUTH-001: too many attempts are explained', async () => {
    fakeApi({ 'POST /session': failure(429, 'rate_limited') });
    const screen = await render(SignInForm, { onsignedin: vi.fn() });
    await screen.getByLabelText(label('Username or email')).fill('admin');
    await screen.getByLabelText(label('Password')).fill('guess');
    await screen.getByRole('button', { name: 'Sign in' }).click();
    await expect
      .element(screen.getByText('Too many failed attempts. Try again in a few minutes.'))
      .toBeVisible();
  });
});

describe('SetupForm', () => {
  it('ADM-047: checks the form, shows the server problems by field, then finishes', async () => {
    let attempts = 0;
    const calls = fakeApi({
      'POST /setup': () =>
        ++attempts === 1
          ? failure(400, 'invalid_request', [
              { path: 'code', code: 'incorrect', message: 'x' },
              { path: 'email', code: 'invalid', message: 'x' },
            ])
          : { status: 201, body: SESSION },
    });
    const ondone = vi.fn();
    const screen = await render(SetupForm, { ondone });

    await expect.element(screen.getByLabelText(label('Username'))).toHaveValue('admin');
    await expect.element(screen.getByLabelText(label('Address'))).toHaveValue(location.origin);
    await screen.getByLabelText(label('Setup code')).fill('abcd-efgh-2345');
    await screen.getByLabelText(label('Full name')).fill('Anna Admin');
    await screen.getByLabelText(label('Email')).fill('anna@example');
    await screen.getByLabelText(label('Password')).fill('correct horse');
    await screen.getByLabelText(label('Repeat the password')).fill('correct hose');
    await screen.getByRole('button', { name: 'Finish setup' }).click();
    await expect.element(screen.getByText('The passwords do not match.')).toBeVisible();
    expect(calls).toEqual([]);

    await screen.getByLabelText(label('Repeat the password')).fill('correct horse');
    await screen.getByRole('button', { name: 'Finish setup' }).click();
    await expect.element(screen.getByText('The setup code is not correct.')).toBeVisible();
    await expect.element(screen.getByText('The value is not valid.')).toBeVisible();
    await expect.element(screen.getByLabelText(label('Setup code'))).toHaveFocus();

    await screen.getByLabelText(label('Setup code')).fill('ABCD-EFGH-2346');
    await screen.getByLabelText(label('Email')).fill('anna@example.com');
    await screen.getByRole('button', { name: 'Finish setup' }).click();
    await vi.waitFor(() => {
      expect(ondone).toHaveBeenCalledOnce();
    });
    expect(calls.at(-1)?.body).toMatchObject({
      code: 'ABCD-EFGH-2346',
      username: 'admin',
      full_name: 'Anna Admin',
      email: 'anna@example.com',
      password: 'correct horse',
      system_language: 'en',
      base_url: location.origin,
      time_zone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    });
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });
});

describe('PasswordForm', () => {
  it('AUTH-009: a wrong current password is reported on its field', async () => {
    setSessionForTests(SESSION);
    let attempts = 0;
    const calls = fakeApi({
      'POST /users/me/password': () =>
        ++attempts === 1
          ? failure(400, 'invalid_request', [
              { path: 'current_password', code: 'incorrect', message: 'x' },
            ])
          : { status: 204 },
    });
    const screen = await render(PasswordForm);
    await screen.getByLabelText(label('Current password')).fill('nope');
    await screen.getByLabelText(label('New password')).fill('brand new pass');
    await screen.getByLabelText(label('Repeat the new password')).fill('brand new pass');
    await screen.getByRole('button', { name: 'Change password' }).click();
    await expect.element(screen.getByText('The current password is not correct.')).toBeVisible();

    await screen.getByLabelText(label('Current password')).fill('correct horse');
    await screen.getByRole('button', { name: 'Change password' }).click();
    await expect.element(screen.getByLabelText(label('Current password'))).toHaveValue('');
    expect(calls.at(-1)?.body).toEqual({
      current_password: 'correct horse',
      new_password: 'brand new pass',
    });
    await expectAccessible(screen.container);
  });
});

describe('TokenList', () => {
  it('AUTH-029, AUTH-030: creates a token, shows its secret once, and deletes a token', async () => {
    let tokens = [token(7, 'CI', ['tracker.write'])];
    const calls = fakeApi({
      'GET /users/me/tokens': () => ({ body: { items: tokens, next_cursor: null } }),
      'POST /users/me/tokens': ({ body }) => {
        const created = token(8, 'Reports', (body as { scopes: ApiToken['scopes'] }).scopes);
        tokens = [created, ...tokens];
        return { status: 201, body: { token: created, secret: 'tfp_secret-value' } };
      },
      'DELETE /users/me/tokens/7': () => {
        tokens = tokens.filter((item) => item.id !== 7);
        return { status: 204 };
      },
    });
    const list = await ensureTokens();
    const screen = await render(TokenList, { tokens: list });

    await expect.element(screen.getByText('CI')).toBeVisible();
    await expect.element(screen.getByText('Tracker: read and change')).toBeVisible();
    await expect.element(screen.getByText('never used', { exact: false })).toBeVisible();

    await screen.getByRole('button', { name: 'Create token' }).click();
    const dialog = page.getByRole('dialog', { name: 'New API token' });
    await dialog.getByLabelText(label('Name')).fill('Reports');
    // Tracker stays "Read"; administration gets "Read" too.
    await dialog
      .getByRole('group', { name: 'Administration' })
      .getByLabelText(label('Read'))
      .click();
    await expectAccessible(dialog.element());
    await dialog.getByRole('button', { name: 'Create token' }).click();

    const created = page.getByRole('dialog', { name: 'Token created' });
    await expect.element(created.getByLabelText(label('Token'))).toHaveValue('tfp_secret-value');
    await expect.element(created.getByRole('button', { name: 'Copy token' })).toBeVisible();
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({
      name: 'Reports',
      scopes: ['tracker.read', 'admin.read'],
    });
    await created.getByRole('button', { name: 'Done' }).click();
    await expect.element(screen.getByText('Reports')).toBeVisible();

    await screen.getByRole('button', { name: 'Delete token “CI”' }).click();
    const confirm = page.getByRole('dialog', { name: 'Delete token “CI”?' });
    await confirm.getByRole('button', { name: 'Delete token' }).click();
    await expect.element(screen.getByText('CI', { exact: true })).not.toBeInTheDocument();
    expect(calls.some((call) => call.method === 'DELETE')).toBe(true);
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('AUTH-030: a token needs access to at least one area', async () => {
    fakeApi({ 'GET /users/me/tokens': { body: { items: [], next_cursor: null } } });
    const list = await ensureTokens();
    await list.refresh();
    const screen = await render(TokenList, { tokens: list });
    await expect.element(screen.getByText('No tokens yet')).toBeVisible();
    await screen.getByRole('button', { name: 'Create token' }).click();
    const dialog = page.getByRole('dialog', { name: 'New API token' });
    await dialog.getByLabelText(label('Name')).fill('Nothing');
    await dialog.getByRole('group', { name: 'Tracker' }).getByLabelText(label('No access')).click();
    await dialog.getByRole('button', { name: 'Create token' }).click();
    await expect.element(dialog.getByText('Choose access to at least one area.')).toBeVisible();
    await userEvent.keyboard('{Escape}');
  });
});
