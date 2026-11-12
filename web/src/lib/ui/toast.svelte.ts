// Feedback toasts (UI-038): short confirmations, errors with retry, undo for reversible actions.
// `Toaster.svelte` shows them; every toast is also announced to screen readers. Errors stay until
// dismissed; other toasts close after a while, and the timers pause while the pointer or keyboard
// focus is on the toasts (WCAG 2.2.1).
import * as common from '#lib/i18n/messages/common.ts';
import { errorText } from '#lib/i18n/messages/errors.ts';

import { announce } from './announce.svelte.ts';

export type ToastTone = 'info' | 'success' | 'error';
export type CloseReason = 'timeout' | 'dismissed' | 'undone' | 'retried' | 'replaced' | 'acted';

/** A button of the toast that leads somewhere, such as «Open» after creating something. */
export interface ToastAction {
  label: string;
  run: () => unknown;
}

export interface ToastOptions {
  message: string;
  tone?: ToastTone;
  /** A second, quieter line: details of an error, a request ID. */
  detail?: string;
  /** Offers Undo while the toast is shown; the action is final once it closes otherwise. */
  undo?: () => unknown;
  /** Offers Try again (errors). */
  retry?: () => unknown;
  action?: ToastAction;
  /** Milliseconds until it closes by itself; `null` keeps it until dismissed. */
  duration?: number | null;
  onClose?: (reason: CloseReason) => void;
}

export interface Toast {
  readonly id: number;
  readonly message: string;
  readonly tone: ToastTone;
  readonly detail: string | undefined;
  readonly undo: (() => unknown) | undefined;
  readonly retry: (() => unknown) | undefined;
  readonly action: ToastAction | undefined;
}

interface Entry extends Toast {
  onClose: ((reason: CloseReason) => void) | undefined;
  remaining: number | null;
  startedAt: number;
  timer: ReturnType<typeof setTimeout> | undefined;
}

export const DEFAULT_DURATION = 5000;
export const UNDO_DURATION = 8000;
export const MAX_TOASTS = 4;

class Toaster {
  // Entries are replaced, never mutated in place, except for their (non-reactive) timers.
  #entries = $state.raw<Entry[]>([]);
  #nextId = 1;
  #paused = false;

  /** Toasts on screen, oldest first. */
  get items(): readonly Toast[] {
    return this.#entries;
  }

  show(options: ToastOptions): number {
    const tone = options.tone ?? 'info';
    const duration =
      options.duration !== undefined
        ? options.duration
        : tone === 'error'
          ? null
          : options.undo
            ? UNDO_DURATION
            : DEFAULT_DURATION;
    const entry: Entry = {
      id: this.#nextId++,
      message: options.message,
      tone,
      detail: options.detail,
      undo: options.undo,
      retry: options.retry,
      action: options.action,
      onClose: options.onClose,
      remaining: duration,
      startedAt: 0,
      timer: undefined,
    };
    this.#entries = [...this.#entries, entry];
    while (this.#entries.length > MAX_TOASTS) {
      const oldest = this.#entries[0];
      if (oldest) this.#close(oldest.id, 'replaced');
    }
    if (!this.#paused) this.#start(entry);
    announce(options.message, tone === 'error' ? 'assertive' : 'polite');
    return entry.id;
  }

  dismiss(id: number): void {
    this.#close(id, 'dismissed');
  }

  /** Runs the toast's undo and closes it. */
  async undo(id: number): Promise<void> {
    const entry = this.#find(id);
    if (!entry?.undo) return;
    this.#close(id, 'undone');
    await entry.undo();
    announce(common.undone());
  }

  /** Closes the toast and runs its action. */
  async act(id: number): Promise<void> {
    const entry = this.#find(id);
    if (!entry?.action) return;
    this.#close(id, 'acted');
    await entry.action.run();
  }

  /** Closes the toast and runs its retry. */
  async retry(id: number): Promise<void> {
    const entry = this.#find(id);
    if (!entry?.retry) return;
    this.#close(id, 'retried');
    await entry.retry();
  }

  /** Stops the countdowns while the user is reading or reaching for a button. */
  pause(): void {
    if (this.#paused) return;
    this.#paused = true;
    const now = performance.now();
    for (const entry of this.#entries) {
      clearTimeout(entry.timer);
      entry.timer = undefined;
      if (entry.remaining !== null) entry.remaining -= now - entry.startedAt;
    }
  }

  resume(): void {
    if (!this.#paused) return;
    this.#paused = false;
    for (const entry of this.#entries) this.#start(entry);
  }

  /** Closes every toast as if dismissed: a menu or popover opens where toasts could cover it. */
  dismissAll(): void {
    for (const entry of [...this.#entries]) this.#close(entry.id, 'dismissed');
  }

  /** Closes everything without callbacks (tests, sign-out). */
  clear(): void {
    for (const entry of this.#entries) clearTimeout(entry.timer);
    this.#entries = [];
    this.#paused = false;
  }

  #find(id: number): Entry | undefined {
    return this.#entries.find((entry) => entry.id === id);
  }

  #start(entry: Entry): void {
    if (entry.remaining === null) return;
    entry.startedAt = performance.now();
    entry.timer = setTimeout(
      () => {
        this.#close(entry.id, 'timeout');
      },
      Math.max(entry.remaining, 0),
    );
  }

  #close(id: number, reason: CloseReason): void {
    const entry = this.#find(id);
    if (!entry) return;
    clearTimeout(entry.timer);
    this.#entries = this.#entries.filter((other) => other.id !== id);
    // The list unmounts when empty, without a pointer-leave; start the next toast's timer anyway.
    if (this.#entries.length === 0) this.#paused = false;
    entry.onClose?.(reason);
  }
}

export const toasts = new Toaster();

function isKnownCode(code: unknown): code is keyof typeof errorText {
  return typeof code === 'string' && Object.hasOwn(errorText, code);
}

/** Text for any failure: the server's code translated, a network failure, or a generic one. */
export function describeError(error: unknown): { message: string; requestId: string | null } {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const code = error.code;
    if (isKnownCode(code)) {
      const requestId = 'requestId' in error ? error.requestId : null;
      return {
        message: errorText[code](),
        requestId: typeof requestId === 'string' ? requestId : null,
      };
    }
  }
  return { message: errorText.internal(), requestId: null };
}

/** Shows a failure as an error toast, with Try again when `retry` is given. */
export function showError(error: unknown, retry?: () => unknown): number {
  const { message, requestId } = describeError(error);
  return toasts.show({
    tone: 'error',
    message,
    detail: requestId ? common.requestId({ id: requestId }) : undefined,
    retry,
  });
}
