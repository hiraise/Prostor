// Whether the navigation rail is collapsed, and how wide it stands expanded: personal settings
// kept on the server (UI-018). Without a choice the rail starts collapsed on medium widths, 16 rem
// wide expanded.
import { MediaQuery } from 'svelte/reactivity';

import { preferences } from '#lib/session/index.ts';

const medium = new MediaQuery('(width < 64rem)', false);

export function isRailCollapsed(): boolean {
  return preferences.chosen('rail_collapsed') ?? medium.current;
}

export function setRailCollapsed(value: boolean): void {
  preferences.set({ rail_collapsed: value });
}

/** Limits of the expanded rail's width, rem × 10 — the range every panel width shares. */
export const RAIL_WIDTH_MIN = 160;
export const RAIL_WIDTH_MAX = 400;

export function railWidth(): number {
  return preferences.value('rail_width');
}

export function setRailWidth(value: number): void {
  preferences.set({ rail_width: value });
}
