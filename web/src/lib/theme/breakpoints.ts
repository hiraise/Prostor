// Layout breakpoints (docs/design.md §6). CSS writes the same widths as media queries.

/** Phones and narrow windows: top bar with a navigation drawer, dialogs as bottom sheets. */
export const NARROW = '(width < 48rem)';
/** Board panels become overlays before the backlog can squeeze the columns. */
export const BOARD_OVERLAY = '(width <= 64rem)';
/** A short touch landscape keeps the phone board even when its width crosses NARROW. */
export const PHONE_LANDSCAPE = '(pointer: coarse) and (max-height: 31.25rem) and (width < 64rem)';
