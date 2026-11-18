// Addresses of signing in and the first-run setup, and where to go after signing in (UI-004).
import { resolve } from '$app/paths';

import type { EndReason } from './session.svelte.ts';

/** The sign-in page; `next` is where to return afterwards, `reason` why the person is there. */
export function signInHref(next?: string, reason?: EndReason): string {
  const params = new URLSearchParams();
  const target = next === undefined ? null : safeNext(next);
  if (target && target !== '/') params.set('next', target);
  if (reason) params.set('reason', reason);
  const query = params.toString();
  return query ? `${resolve('/(public)/signin')}?${query}` : resolve('/(public)/signin');
}

/** Why the sign-in page was opened, from its `reason` parameter. */
export function endReason(raw: string | null): EndReason | null {
  return raw === 'signed-out' || raw === 'expired' ? raw : null;
}

export function setupHref(): string {
  return resolve('/(public)/setup');
}

/** The page where a new password must be chosen before anything else (AUTH-010). */
export function changePasswordHref(next?: string): string {
  const target = next === undefined ? null : safeNext(next);
  const base = resolve('/(public)/change-password');
  return target && target !== '/' ? `${base}?next=${encodeURIComponent(target)}` : base;
}

/** The page that asks for a password-reset link (AUTH-008). */
export function forgotPasswordHref(): string {
  return resolve('/(public)/forgot-password');
}

/**
 * A path of this app to return to, or `null`. Refuses anything that would leave the app:
 * absolute URLs, `//host` and backslash tricks, and the sign-in and setup pages themselves.
 */
export function safeNext(raw: string | null): string | null {
  if (!raw?.startsWith('/') || raw.startsWith('//') || raw.includes('\\')) return null;
  let url: URL;
  try {
    url = new URL(raw, 'http://taskmaster.invalid');
  } catch {
    return null;
  }
  if (url.origin !== 'http://taskmaster.invalid') return null;
  const outside: string[] = [
    resolve('/(public)/signin'),
    resolve('/(public)/setup'),
    resolve('/(public)/change-password'),
  ];
  if (outside.includes(url.pathname)) return null;
  return url.pathname + url.search + url.hash;
}
