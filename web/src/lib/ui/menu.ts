// The types of the menu (WAI-ARIA menu pattern) — here, not in `Menu.svelte`'s module script,
// so plain modules can import them too (step 2.2 §7's create-menu entries).
import type { LucideIcon } from '@lucide/svelte';

export interface MenuAction {
  type: 'action';
  label: string;
  icon?: LucideIcon;
  onSelect: () => void;
  disabled?: boolean;
  danger?: boolean;
}

/** One choice of a set; the checked one shows a check mark, not only a color (UI-032). */
export interface MenuRadio {
  type: 'radio';
  label: string;
  checked: boolean;
  onSelect: () => void;
  /** Language of the label, when it differs from the page (language names). */
  lang?: string;
}

export interface MenuGroup {
  type: 'group';
  label: string;
  items: (MenuAction | MenuRadio)[];
}

export interface MenuSeparator {
  type: 'separator';
}

export type MenuEntry = MenuAction | MenuRadio | MenuGroup | MenuSeparator;

/** Props the trigger button must spread: ARIA state, popover link and the anchor reference. */
export type MenuTriggerProps = Record<string | symbol, unknown>;
