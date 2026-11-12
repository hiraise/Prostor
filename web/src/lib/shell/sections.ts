// Sections of the main navigation (UI-001). A section is listed here when its first screen exists;
// a section that needs permissions or a capability has a visibility rule, evaluated per person.
import type { LucideIcon } from '@lucide/svelte';
import FolderKanban from '@lucide/svelte/icons/folder-kanban';
import ListChecks from '@lucide/svelte/icons/list-checks';
import ShieldCheck from '@lucide/svelte/icons/shield-check';
import SquareKanban from '@lucide/svelte/icons/square-kanban';
import { resolve } from '$app/paths';

import { canAdminister, preferences } from '#lib/session/index.ts';

import * as m from './messages.ts';

export interface Section {
  id: 'issues' | 'boards' | 'projects' | 'admin';
  href: string;
  icon: LucideIcon;
  label: () => string;
  /** Shown only when this holds; reactive. Without it the section is always shown. */
  visible?: () => boolean;
}

export const sections: readonly Section[] = [
  { id: 'issues', href: resolve('/(app)/issues'), icon: ListChecks, label: m.sectionIssues },
  { id: 'boards', href: resolve('/(app)/boards'), icon: SquareKanban, label: m.sectionBoards },
  {
    id: 'projects',
    href: resolve('/(app)/projects'),
    icon: FolderKanban,
    label: m.sectionProjects,
  },
  {
    id: 'admin',
    href: resolve('/(app)/admin'),
    icon: ShieldCheck,
    label: m.sectionAdmin,
    visible: canAdminister,
  },
];

/** The sections this person may use. Reactive. */
export function visibleSections(): Section[] {
  return sections.filter((section) => section.visible?.() ?? true);
}

/** Where the logo and "home" lead: the person's home page (UI-004), with the instance's own
 * default (set in «Настройки») standing in until they choose one. */
export function homeHref(): string {
  const home = preferences.value('home');
  if (home === 'projects') return resolve('/(app)/projects');
  if (home === 'boards') return resolve('/(app)/boards');
  return resolve('/(app)/issues');
}

/** How a navigation link relates to the current page: the page itself, inside it, or neither. */
export function linkState(href: string, pathname: string): 'page' | 'true' | undefined {
  if (pathname === href) return 'page';
  if (pathname.startsWith(`${href}/`)) return 'true';
  return undefined;
}
