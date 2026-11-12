// The session module: loading once, signing in and out, sessions the server ended, and where to
// return after signing in (AUTH-001, AUTH-005, UI-004).
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiError, createApiClient, type ClientOptions } from '#lib/api/client.ts';
import type { Session, paths } from '#lib/api/schema.gen.ts';

import {
  currentUser,
  endReason,
  ensureSession,
  getSession,
  onSessionEnd,
  passwordChanged,
  safeNext,
  setSessionForTests,
  signIn,
  signInHref,
  signOut,
  updateCurrentUser,
} from './index.ts';

const SESSION: Session = {
  user: {
    id: 1,
    username: 'anna',
    full_name: 'Anna',
    email: null,
    email_verified: false,
    avatar_color: 0,
    avatar_url: null,
    version: 1,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

function client(handler: (method: string, path: string) => Response, options: ClientOptions = {}) {
  const fetcher = vi.fn((input: RequestInfo | URL, init?: RequestInit) =>
    Promise.resolve(
      handler(init?.method ?? 'GET', input instanceof Request ? input.url : input.toString()),
    ),
  );
  return { api: createApiClient<paths>({ fetch: fetcher, ...options }), fetcher };
}

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

afterEach(() => {
  setSessionForTests(null);
});

describe('session', () => {
  it('AUTH-001: loads the session once and remembers that nobody is signed in', async () => {
    const { api, fetcher } = client(() =>
      json(401, { code: 'unauthenticated', message: 'no session' }),
    );
    setSessionForTests(undefined, api);
    await expect(ensureSession()).resolves.toBeNull();
    await expect(ensureSession()).resolves.toBeNull();
    expect(getSession()).toBeNull();
    expect(fetcher).toHaveBeenCalledOnce();
  });

  it('AUTH-001: signing in keeps the session; AUTH-005: signing out ends it', async () => {
    const { api } = client((method) =>
      method === 'POST' ? json(201, SESSION) : new Response(null, { status: 204 }),
    );
    setSessionForTests(null, api);
    const ended = vi.fn();
    const stop = onSessionEnd(ended);
    await signIn({ login: 'anna', password: 'correct horse', remember: false });
    expect(currentUser()?.username).toBe('anna');
    await expect(ensureSession()).resolves.toEqual(SESSION);
    await signOut();
    expect(getSession()).toBeNull();
    expect(ended).toHaveBeenCalledWith('signed-out');
    stop();
  });

  it('AUTH-001: a failed sign-in throws the server error', async () => {
    const { api } = client(() => json(401, { code: 'invalid_credentials', message: 'no' }));
    setSessionForTests(null, api);
    await expect(signIn({ login: 'anna', password: 'x', remember: false })).rejects.toBeInstanceOf(
      ApiError,
    );
    expect(getSession()).toBeNull();
  });
});

describe('a required new password', () => {
  it('AUTH-010: once chosen, later loads of the session no longer ask for it', async () => {
    setSessionForTests({ ...SESSION, password_change_required: true });
    passwordChanged();
    // Regression: the cached load still answered with the old session, so every page went back
    // to the password form.
    await expect(ensureSession()).resolves.toMatchObject({ password_change_required: false });
    expect(getSession()?.password_change_required).toBe(false);
    updateCurrentUser({ ...SESSION.user, full_name: 'Anna K' });
    await expect(ensureSession()).resolves.toMatchObject({ user: { full_name: 'Anna K' } });
  });
});

describe('addresses', () => {
  it('UI-004: returns only to paths of this app', () => {
    expect(safeNext('/issues?q=open#top')).toBe('/issues?q=open#top');
    for (const bad of [
      null,
      '',
      'issues',
      '//evil.example/x',
      '/\\evil.example',
      'https://evil.example',
      '/signin',
      '/setup?x=1',
    ]) {
      expect(safeNext(bad), String(bad)).toBeNull();
    }
  });

  it('builds the sign-in address with where to return and why', () => {
    expect(signInHref()).toBe('/signin');
    expect(signInHref('/boards/3')).toBe('/signin?next=%2Fboards%2F3');
    expect(signInHref('//evil.example')).toBe('/signin');
    expect(signInHref('/issues', 'expired')).toBe('/signin?next=%2Fissues&reason=expired');
    expect(signInHref(undefined, 'signed-out')).toBe('/signin?reason=signed-out');
    expect(endReason('expired')).toBe('expired');
    expect(endReason('whatever')).toBeNull();
  });
});
