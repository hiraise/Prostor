// Messages for screen readers (UI-032): results of actions, loaded pages, errors. `Announcer.svelte`
// renders the two live regions once; anything can call `announce`.

export type Politeness = 'polite' | 'assertive';

const regions = $state<Record<Politeness, string>>({ polite: '', assertive: '' });
// Screen readers announce changes only; a non-breaking space toggled at the end makes a repeated
// message a change.
let toggle = false;

export function announce(message: string, politeness: Politeness = 'polite'): void {
  toggle = !toggle;
  regions[politeness] = toggle ? `${message}\u00a0` : message;
}

/** Current text of a live region (read by `Announcer.svelte` and tests). */
export function announcement(politeness: Politeness): string {
  return regions[politeness];
}
