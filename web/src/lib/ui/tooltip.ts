// Tooltips as an attachment: `<button aria-label={label} {@attach tooltip(label)}>`.
// A tooltip only repeats what the element already tells assistive technology (its name or
// description), so it never carries information of its own. One shared element in the top layer
// serves every tooltip. Behavior follows WCAG 1.4.13: shown on hover (after a delay) or keyboard
// focus, stays while the pointer is over it, hides on Escape without moving focus.
import type { Attachment } from 'svelte/attachments';

import { placeElement, type Placement } from './position.ts';
import './tooltip.css';

const SHOW_DELAY = 500;
/** After one tooltip hides, the next one shows at once for this long (moving along a toolbar). */
const WARM_WINDOW = 400;
const HIDE_GRACE = 100;

let host: HTMLDivElement | null = null;
let active: HTMLElement | null = null;
let showTimer: ReturnType<typeof setTimeout> | undefined;
let hideTimer: ReturnType<typeof setTimeout> | undefined;
let hiddenAt = Number.NEGATIVE_INFINITY;

function ensureHost(): HTMLDivElement {
  if (host?.isConnected) return host;
  const element = document.createElement('div');
  element.className = 'tf-tooltip';
  element.setAttribute('role', 'tooltip');
  element.popover = 'manual';
  element.addEventListener('pointerenter', () => {
    clearTimeout(hideTimer);
  });
  element.addEventListener('pointerleave', scheduleHide);
  document.body.append(element);
  host = element;
  return element;
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key !== 'Escape' || !active) return;
  // The tooltip goes first; a second Escape reaches the dialog or menu underneath.
  event.preventDefault();
  event.stopPropagation();
  hide();
}

function show(target: HTMLElement, text: string, placement: Placement): void {
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  const element = ensureHost();
  element.textContent = text;
  if (!element.matches(':popover-open')) element.showPopover();
  placeElement(element, target, placement);
  if (!active) document.addEventListener('keydown', onKeydown, true);
  active = target;
}

function hide(): void {
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  if (!active) return;
  active = null;
  hiddenAt = performance.now();
  document.removeEventListener('keydown', onKeydown, true);
  if (host?.matches(':popover-open')) host.hidePopover();
}

function scheduleHide(): void {
  clearTimeout(hideTimer);
  hideTimer = setTimeout(hide, HIDE_GRACE);
}

/** The element whose tooltip is visible, if any (for tests). */
export function visibleTooltip(): { target: HTMLElement; text: string } | null {
  return active && host ? { target: active, text: host.textContent } : null;
}

export function tooltip(
  text: string,
  placement: Placement = { side: 'bottom', align: 'center' },
): Attachment<HTMLElement> {
  return (target) => {
    const enter = (event: PointerEvent): void => {
      if (event.pointerType === 'touch') return;
      clearTimeout(hideTimer);
      if (active === target) return;
      clearTimeout(showTimer);
      if (active || performance.now() - hiddenAt < WARM_WINDOW) show(target, text, placement);
      else
        showTimer = setTimeout(() => {
          show(target, text, placement);
        }, SHOW_DELAY);
    };
    const leave = (): void => {
      clearTimeout(showTimer);
      if (active === target) scheduleHide();
    };
    const focus = (): void => {
      // Focus can arrive inside the browser's own popover operation (a closing menu hands focus
      // back to its trigger), where opening another popover is not allowed; show after it ends.
      clearTimeout(showTimer);
      showTimer = setTimeout(() => {
        if (document.activeElement === target && target.matches(':focus-visible')) {
          show(target, text, placement);
        }
      }, 0);
    };
    const blur = (): void => {
      if (active === target) hide();
    };
    const press = (): void => {
      clearTimeout(showTimer);
      if (active === target) hide();
    };

    target.addEventListener('pointerenter', enter);
    target.addEventListener('pointerleave', leave);
    target.addEventListener('focus', focus);
    target.addEventListener('blur', blur);
    target.addEventListener('pointerdown', press);
    return () => {
      target.removeEventListener('pointerenter', enter);
      target.removeEventListener('pointerleave', leave);
      target.removeEventListener('focus', focus);
      target.removeEventListener('blur', blur);
      target.removeEventListener('pointerdown', press);
      if (active === target) hide();
      clearTimeout(showTimer);
    };
  };
}
