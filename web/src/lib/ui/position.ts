// Where to put a floating element (menu, tooltip) next to its anchor so it stays in the viewport.
// Pure geometry; CSS anchor positioning would do this, but Safari 17.4 lacks it (UI-035).

export type Side = 'top' | 'bottom' | 'left' | 'right';
export type Align = 'start' | 'center' | 'end';
export interface Placement {
  side: Side;
  align: Align;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface Size {
  width: number;
  height: number;
}

const OPPOSITE: Record<Side, Side> = { top: 'bottom', bottom: 'top', left: 'right', right: 'left' };
/** Distance kept from the viewport edges. */
const MARGIN = 8;

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

function fits(side: Side, anchor: Rect, floating: Size, viewport: Size, gap: number): boolean {
  switch (side) {
    case 'top':
      return anchor.y - gap - floating.height >= MARGIN;
    case 'bottom':
      return anchor.y + anchor.height + gap + floating.height <= viewport.height - MARGIN;
    case 'left':
      return anchor.x - gap - floating.width >= MARGIN;
    case 'right':
      return anchor.x + anchor.width + gap + floating.width <= viewport.width - MARGIN;
  }
}

function alongAxis(start: number, length: number, size: number, align: Align): number {
  if (align === 'start') return start;
  if (align === 'end') return start + length - size;
  return start + (length - size) / 2;
}

/**
 * Top-left corner for `floating` placed on `preferred.side` of `anchor`, flipped to the opposite
 * side when it does not fit there but fits on the other, then shifted to stay inside the viewport.
 */
export function place(
  anchor: Rect,
  floating: Size,
  viewport: Size,
  preferred: Placement,
  gap = 6,
): { x: number; y: number; side: Side } {
  let side = preferred.side;
  if (
    !fits(side, anchor, floating, viewport, gap) &&
    fits(OPPOSITE[side], anchor, floating, viewport, gap)
  ) {
    side = OPPOSITE[side];
  }

  let x: number;
  let y: number;
  if (side === 'top' || side === 'bottom') {
    y = side === 'top' ? anchor.y - gap - floating.height : anchor.y + anchor.height + gap;
    x = alongAxis(anchor.x, anchor.width, floating.width, preferred.align);
  } else {
    x = side === 'left' ? anchor.x - gap - floating.width : anchor.x + anchor.width + gap;
    y = alongAxis(anchor.y, anchor.height, floating.height, preferred.align);
  }

  return {
    x: clamp(x, MARGIN, viewport.width - floating.width - MARGIN),
    y: clamp(y, MARGIN, viewport.height - floating.height - MARGIN),
    side,
  };
}

/** Moves a `position: fixed` element next to its anchor (call after it is visible and sized). */
export function placeElement(
  element: HTMLElement,
  anchor: Element,
  preferred: Placement,
  gap?: number,
): Side {
  const box = element.getBoundingClientRect();
  const { x, y, side } = place(
    anchor.getBoundingClientRect(),
    { width: box.width, height: box.height },
    { width: document.documentElement.clientWidth, height: window.innerHeight },
    preferred,
    gap,
  );
  element.style.left = `${String(Math.round(x))}px`;
  element.style.top = `${String(Math.round(y))}px`;
  element.dataset.side = side;
  return side;
}
