// Emailed one-time links (AUTH-008, AUTH-014, ACC-004). The secret is the part of the address
// after `#`, which browsers never send to a server; the page reads it once and removes it from the
// address bar and history, so it does not linger where others could see it.
import { ApiError, api } from '#lib/api/client.ts';
import type { LinkInfo, Session } from '#lib/api/schema.gen.ts';

/** The secret in the current address, removed from the address afterwards. */
export function takeSecret(): string | null {
  const secret = decodeURIComponent(location.hash.slice(1)).trim();
  if (location.hash) history.replaceState(history.state, '', location.pathname + location.search);
  return secret || null;
}

/** What a link is for: `invalid` when unknown or used. */
export async function inspectLink(token: string): Promise<LinkInfo | 'invalid'> {
  try {
    return await api.post('/account-links/inspect', { body: { token } });
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_found') return 'invalid';
    throw error;
  }
}

export function requestReset(email: string): Promise<unknown> {
  return api.post('/password-reset', { body: { email } });
}

export function confirmReset(token: string, newPassword: string): Promise<Session> {
  return api.post('/password-reset/confirm', { body: { token, new_password: newPassword } });
}

export function acceptInvitation(
  token: string,
  password: string,
  fullName: string,
  username: string,
): Promise<Session> {
  return api.post('/invitations/accept', {
    body: { token, password, full_name: fullName, username },
  });
}

export async function confirmEmail(token: string): Promise<string> {
  const answer = await api.post('/email-verification/confirm', { body: { token } });
  return answer.email;
}
