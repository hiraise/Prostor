// Copy actions (copy ID, copy link — UI-006) with a confirmation toast, or an error toast telling
// the user to copy by hand when the browser refuses.
import * as common from '#lib/i18n/messages/common.ts';

import { toasts } from './toast.svelte.ts';

export async function copyText(
  text: string,
  confirmation: string = common.copied(),
): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    toasts.show({ tone: 'success', message: confirmation });
    return true;
  } catch {
    toasts.show({ tone: 'error', message: common.copyFailed(), detail: text });
    return false;
  }
}

/** Copies the absolute address of `href` (default: the current page). */
export function copyLink(href: string = location.href): Promise<boolean> {
  return copyText(new URL(href, location.href).href, common.linkCopied());
}
