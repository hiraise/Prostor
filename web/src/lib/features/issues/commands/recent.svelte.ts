// The last applied command (plan 2.1 §6, ISS-017's remainder): «Повторить последнюю команду»
// pre-fills it, and reopening the dialog shows it. The full history lives on the server
// (migration 0022) and reaches the dialog as the completion's `recent` group; this keeps the
// last line of this tab, so the dialog opens before any request answers.

let lastApplied = $state<string | null>(null);

export const recentCommands = {
  /** Remembers a line the person applied. */
  remember(text: string): void {
    const trimmed = text.trim();
    if (trimmed !== '') lastApplied = trimmed;
  },

  /** The last applied line of this tab, or `null` when none. */
  last(): string | null {
    return lastApplied;
  },

  /** For tests. */
  reset(): void {
    lastApplied = null;
  },
};
