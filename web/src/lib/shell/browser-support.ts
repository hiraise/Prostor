// The oldest browsers we support (UI-035): Baseline 2024, the same floor as the build targets in
// vite.config.ts. Each check is a feature the app relies on and that arrived in that baseline or
// earlier; a browser missing any of them gets a non-blocking notice.

const checks: readonly (readonly [string, () => boolean])[] = [
  ['Object.groupBy', () => typeof Object.groupBy === 'function'],
  ['Promise.withResolvers', () => typeof Promise.withResolvers === 'function'],
  ['popover', () => Object.hasOwn(HTMLElement.prototype, 'popover')],
  ['dialog', () => typeof HTMLDialogElement === 'function'],
  [':has()', () => CSS.supports('selector(:has(*))')],
  ['oklch()', () => CSS.supports('color', 'oklch(50% 0.1 200)')],
  ['dvh', () => CSS.supports('height', '100dvh')],
  ['media ranges', () => matchMedia('(width >= 0px)').matches],
];

/** Names of the missing features; empty in a supported browser. */
export function missingFeatures(): string[] {
  return checks
    .filter(([, supported]) => {
      try {
        return !supported();
      } catch {
        return true;
      }
    })
    .map(([name]) => name);
}
