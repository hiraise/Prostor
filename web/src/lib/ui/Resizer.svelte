<script lang="ts">
  // A handle between a panel and the content that changes the panel's width: drag it, or focus it
  // and use the arrow keys (1 rem a step), Home and End. Widths are in tenths of a rem, as kept in
  // personal settings. `onchange` follows the drag; `done` is set once it ends (or for a key).
  interface Props {
    /** The panel's width, rem × 10. */
    value: number;
    min: number;
    max: number;
    /** Where the panel is: at the end, moving the handle towards the start widens it. */
    side: 'start' | 'end';
    label: string;
    onchange: (value: number, done: boolean) => void;
  }

  let { value, min, max, side, label, onchange }: Props = $props();

  /** One arrow key press: 1 rem. */
  const STEP = 10;

  let drag: { x: number; from: number; last: number } | null = null;

  function clamp(width: number): number {
    return Math.min(max, Math.max(min, Math.round(width)));
  }

  /** +1 when moving the handle to the right widens the panel. */
  function direction(element: HTMLElement): number {
    const rtl = getComputedStyle(element).direction === 'rtl';
    return (side === 'start') !== rtl ? 1 : -1;
  }

  function keydown(event: KeyboardEvent): void {
    const element = event.currentTarget as HTMLElement;
    let next: number | null = null;
    if (event.key === 'ArrowRight') next = value + STEP * direction(element);
    if (event.key === 'ArrowLeft') next = value - STEP * direction(element);
    if (event.key === 'Home') next = min;
    if (event.key === 'End') next = max;
    if (next === null) return;
    event.preventDefault();
    onchange(clamp(next), true);
  }

  function pointerdown(event: PointerEvent): void {
    if (event.button !== 0) return;
    const element = event.currentTarget as HTMLElement;
    element.setPointerCapture(event.pointerId);
    drag = { x: event.clientX, from: value, last: value };
  }

  function pointermove(event: PointerEvent): void {
    if (!drag) return;
    const element = event.currentTarget as HTMLElement;
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    const moved = ((event.clientX - drag.x) / rem) * 10 * direction(element);
    drag.last = clamp(drag.from + moved);
    onchange(drag.last, false);
  }

  function pointerup(): void {
    if (!drag) return;
    const last = drag.last;
    drag = null;
    onchange(last, true);
  }
</script>

<!-- A focusable separator is an interactive widget (WAI-ARIA window splitter). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="resizer"
  role="separator"
  tabindex="0"
  aria-label={label}
  aria-orientation="vertical"
  aria-valuenow={value}
  aria-valuemin={min}
  aria-valuemax={max}
  onkeydown={keydown}
  onpointerdown={pointerdown}
  onpointermove={pointermove}
  onpointerup={pointerup}
  onpointercancel={pointerup}
></div>

<style>
  .resizer {
    flex: none;
    width: var(--space-2);
    cursor: col-resize;
    touch-action: none;
    position: relative;
  }

  .resizer::after {
    content: '';
    position: absolute;
    inset-block: 0;
    inset-inline-start: calc(50% - 1px);
    width: 2px;
    border-radius: var(--radius-full);
    background: transparent;
  }

  .resizer:hover::after,
  .resizer:focus-visible::after {
    background: var(--color-border-control);
  }
</style>
