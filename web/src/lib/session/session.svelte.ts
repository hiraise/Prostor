// The signed-in person (AUTH-001, AUTH-005, D-023). The session itself is an HTTP-only cookie the
// page never sees; this module knows whether there is one and whose it is. It is loaded once at
// startup and kept in step between the tabs of this browser: signing out in one tab signs out all.
import { api as defaultApi, type ApiClient, onApiError } from '#lib/api/client.ts';
import type {
  CurrentUser,
  Session,
  SetupRequest,
  SignInRequest,
  paths,
} from '#lib/api/schema.gen.ts';

import { clearAccess } from './access.svelte.ts';
import { clearPreferences } from './preferences.svelte.ts';

/** Why the session ended: signed out here or in another tab, or it expired on the server. */
export type EndReason = 'signed-out' | 'expired';

type Message = { type: 'signed-in' } | { type: 'signed-out' };

let current = $state<Session | null>(null);
let loaded: Promise<Session | null> | null = null;
let client: ApiClient<paths> = defaultApi;
let channel: BroadcastChannel | null = null;
// Listeners, not state: nothing renders from these sets.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const endListeners = new Set<(reason: EndReason) => void>();
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const startListeners = new Set<() => void>();
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const changeListeners = new Set<() => void>();

/** The current session; `null` when nobody is signed in. Reactive. */
export function getSession(): Session | null {
  return current;
}

/** The signed-in user; `null` when nobody is signed in. Reactive. */
export function currentUser(): CurrentUser | null {
  return current?.user ?? null;
}

/**
 * The session, loaded from the server the first time and from memory afterwards. `null` when
 * nobody is signed in. A failed load is not remembered, so the next call tries again.
 */
export function ensureSession(fetcher?: typeof fetch): Promise<Session | null> {
  loaded ??= client
    .get('/session', { fetch: fetcher, accept: [401, 403] })
    .then((answer) => {
      const session = 'user' in answer ? answer : null;
      current = session;
      return session;
    })
    .catch((error: unknown) => {
      loaded = null;
      throw error;
    });
  return loaded;
}

/** Signs in with a username or email and a password; other tabs follow. */
export async function signIn(request: SignInRequest): Promise<Session> {
  return started(await client.post('/session', { body: request }));
}

/** Runs the first-run setup, which signs the new administrator in (ADM-047). */
export async function runSetup(request: SetupRequest): Promise<Session> {
  return started(await client.post('/setup', { body: request }));
}

/** Takes over a session the server started (a password reset, an accepted invitation). */
export function adoptSession(session: Session): Session {
  return started(session);
}

/** Shows the signed-in person's new name, username or email after they changed it. */
export function updateCurrentUser(user: CurrentUser): void {
  if (current?.user.id !== user.id) return;
  current = { ...current, user };
  loaded = Promise.resolve(current);
}

/** Marks the session as no longer waiting for a new password (AUTH-010). */
export function passwordChanged(): void {
  if (!current) return;
  current = { ...current, password_change_required: false };
  // Later loads of the session must see the change too.
  loaded = Promise.resolve(current);
}

/** Calls `listener` when the server says a new password must be chosen first (AUTH-010). */
export function onPasswordChangeRequired(listener: () => void): () => void {
  changeListeners.add(listener);
  return () => {
    changeListeners.delete(listener);
  };
}

/** Signs out in this browser: every tab learns about it. */
export async function signOut(): Promise<void> {
  await client.delete('/session');
  ended('signed-out');
  post({ type: 'signed-out' });
}

/** Calls `listener` when the session ends; returns a function that stops listening. */
export function onSessionEnd(listener: (reason: EndReason) => void): () => void {
  endListeners.add(listener);
  return () => {
    endListeners.delete(listener);
  };
}

/** The server ended the session outside a request (the live connection was closed for it). */
export function sessionEndedOnServer(): void {
  if (current) ended('expired');
}

/** Calls `listener` when someone signs in from another tab of this browser. */
export function onSignInElsewhere(listener: () => void): () => void {
  startListeners.add(listener);
  return () => {
    startListeners.delete(listener);
  };
}

/**
 * Starts following sign-ins and sign-outs of other tabs and session ends reported by the server.
 * Call once at startup; returns a function that stops.
 */
export function initSession(): () => void {
  const stopErrors = onApiError((error) => {
    // A request without a valid session: it has ended (expired, revoked, signed out elsewhere).
    if (error.code === 'unauthenticated' && current) ended('expired');
    if (error.code === 'password_change_required') {
      for (const listener of changeListeners) listener();
    }
  });
  if (typeof BroadcastChannel !== 'undefined') {
    channel = new BroadcastChannel('prostor.session');
    channel.onmessage = (event: MessageEvent<Message>) => {
      if (event.data.type === 'signed-out' && current) ended('signed-out');
      if (event.data.type === 'signed-in') {
        // The session belongs to the whole browser: load it again before telling anyone.
        loaded = null;
        clearAccess();
        clearPreferences();
        for (const listener of startListeners) listener();
      }
    };
  }
  return () => {
    stopErrors();
    channel?.close();
    channel = null;
  };
}

function started(session: Session): Session {
  clearAccess();
  clearPreferences();
  current = session;
  loaded = Promise.resolve(session);
  post({ type: 'signed-in' });
  return session;
}

function ended(reason: EndReason): void {
  clearAccess();
  clearPreferences();
  current = null;
  loaded = Promise.resolve(null);
  for (const listener of endListeners) listener(reason);
}

function post(message: Message): void {
  channel?.postMessage(message);
}

/**
 * For tests: a client to use instead of the app's, and a session as if loaded (`undefined`: not
 * loaded yet, so the next `ensureSession` asks the server).
 */
export function setSessionForTests(
  session: Session | null | undefined,
  api?: ApiClient<paths>,
): void {
  current = session ?? null;
  loaded = session === undefined ? null : Promise.resolve(session);
  if (api) client = api;
}
