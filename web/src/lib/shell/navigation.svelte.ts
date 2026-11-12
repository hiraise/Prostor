// Whether the navigation rail is collapsed: a personal setting kept on the server (UI-018). Without
// a choice the rail starts collapsed on medium widths.
import { MediaQuery } from 'svelte/reactivity';

import { preferences } from '#lib/session/index.ts';

const medium = new MediaQuery('(width < 64rem)', false);

export function isRailCollapsed(): boolean {
  return preferences.chosen('rail_collapsed') ?? medium.current;
}

export function setRailCollapsed(value: boolean): void {
  preferences.set({ rail_collapsed: value });
}
