// Checks of a new password before it is sent; the server checks again (AUTH-009).
import { fieldErrorText } from '#lib/i18n/messages/errors.ts';

import * as m from './messages.ts';

/** Problems of a new password typed twice, by field: `path` and `repeat`. */
export function checkNewPassword(
  password: string,
  repeat: string,
  path = 'password',
): Record<string, string> {
  if (!password) return { [path]: fieldErrorText.required() };
  if (Array.from(password).length < 8) return { [path]: m.passwordTooShort() };
  if (password !== repeat) return { repeat: m.passwordsDiffer() };
  return {};
}
