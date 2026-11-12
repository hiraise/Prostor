// How the fields screens word types and refusals: a field's type in the viewer's language
// (FLD-001), and what a form says when the server refuses a change — field errors under their
// inputs, missing permissions by name (FLD-027), a conflict in the action's own words, or the
// text of the error code.
import { ApiError } from '#lib/api/client.ts';
import type { FieldKind, PermissionKey } from '#lib/api/schema.gen.ts';
import { permissions } from '#lib/features/access/index.ts';
import { fieldErrorTexts, type OwnFieldTexts } from '#lib/i18n/messages/errors.ts';
import { describeError } from '#lib/ui/toast.svelte.ts';

import * as m from './messages.ts';

/** A field's type: its kind, and "several values" for multi-valued ones (FLD-001). */
export function typeLabel(field: { kind: FieldKind; multi: boolean }): string {
  const kind = m.kindNames[field.kind]();
  return field.multi ? m.typeWithMany({ kind }) : kind;
}

function isPermission(key: string): key is PermissionKey {
  return Object.hasOwn(permissions, key);
}

/** What a refused change says: texts under inputs by path, and a message for the rest. */
export interface Refusal {
  /** Texts under inputs, by the request's body path. */
  fields: Record<string, string>;
  /** A message for the form as a whole; `null` when the field texts say it all. */
  message: string | null;
}

/**
 * The words for a refused change. `own` replaces the generic texts of field errors (by path and
 * code); `conflict` explains a `conflict` answer for this action; `paths` are the inputs the form
 * shows, so errors of other paths become the message.
 */
export function refusal(
  error: unknown,
  options: { own?: OwnFieldTexts; conflict?: () => string; paths?: readonly string[] } = {},
): Refusal {
  if (error instanceof ApiError) {
    if (error.code === 'invalid_request') {
      const texts = fieldErrorTexts(error.body?.fields ?? [], options.own);
      const shown: Record<string, string> = {};
      const other: string[] = [];
      for (const [path, text] of Object.entries(texts)) {
        if (!options.paths || options.paths.includes(path)) shown[path] = text;
        else other.push(text);
      }
      if (Object.keys(texts).length === 0) {
        return { fields: {}, message: describeError(error).message };
      }
      return { fields: shown, message: other.length > 0 ? other.join(' ') : null };
    }
    const missing = error.body?.missing_permissions ?? [];
    if (error.code === 'forbidden' && missing.length > 0) {
      const names = missing.map((key) => (isPermission(key) ? permissions[key].name() : key));
      return { fields: {}, message: m.missingPermissions({ names: names.join(', ') }) };
    }
    if (error.code === 'conflict' && options.conflict) {
      return { fields: {}, message: options.conflict() };
    }
  }
  return { fields: {}, message: describeError(error).message };
}

/** The refusal as one line, for a notice or a toast. */
export function refusalText(error: unknown, conflict?: () => string): string {
  const found = refusal(error, { conflict, paths: [] });
  return found.message ?? describeError(error).message;
}

/** Moves focus to the first input a form marks invalid. */
export function focusFirstError(form: HTMLElement | undefined): void {
  queueMicrotask(() => {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  });
}
