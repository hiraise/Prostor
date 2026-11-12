// Tokens and themes: every text/background pair the components use meets WCAG 2.2 AA in both
// themes (UI-032), touch screens get 44 px controls (D-019).
import { afterEach, describe, expect, it } from 'vitest';

import { isPhone } from '../../test/a11y.ts';

import { applyTheme, getTheme, setThemePreference, type Theme } from './theme.svelte.ts';

/** The sRGB value of any CSS color, as the browser renders it. */
function rgb(color: string): [number, number, number] {
  const canvas = document.createElement('canvas');
  canvas.width = 1;
  canvas.height = 1;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) throw new Error('no canvas');
  context.fillStyle = color;
  context.fillRect(0, 0, 1, 1);
  const [r = 0, g = 0, b = 0] = context.getImageData(0, 0, 1, 1).data;
  return [r, g, b];
}

function luminance([r, g, b]: [number, number, number]): number {
  const linear = (value: number) => {
    const c = value / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function contrast(foreground: string, background: string): number {
  const a = luminance(rgb(token(foreground)));
  const b = luminance(rgb(token(background)));
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

// [foreground, background, minimum]: 4.5 for text, 3 for control borders and focus rings.
const PAIRS: [string, string, number][] = [
  ...[
    '--color-canvas',
    '--color-surface',
    '--color-surface-sunken',
    '--color-surface-raised',
  ].flatMap((surface): [string, string, number][] => [
    ['--color-text', surface, 4.5],
    ['--color-text-muted', surface, 4.5],
    ['--color-link', surface, 4.5],
    ['--color-focus', surface, 3],
    ['--color-border-control', surface, 3],
  ]),
  ['--color-text', '--color-hover', 4.5],
  ['--color-text', '--color-selected', 4.5],
  ['--color-text-muted', '--color-hover', 4.5],
  ['--color-text-faint', '--color-surface', 4.5],
  ['--color-on-ink', '--color-ink', 4.5],
  ['--color-on-ink', '--color-ink-hover', 4.5],
  ['--color-on-danger', '--color-danger-solid', 4.5],
  ['--color-on-danger', '--color-danger-solid-hover', 4.5],
  ['--color-danger', '--color-danger-bg', 4.5],
  ['--color-danger', '--color-surface', 4.5],
  ['--color-success', '--color-success-bg', 4.5],
  ['--color-warning', '--color-warning-bg', 4.5],
  ['--color-info', '--color-info-bg', 4.5],
  ['--color-text', '--color-danger-bg', 4.5],
  ['--color-text', '--color-warning-bg', 4.5],
  ['--color-surface', '--color-text', 4.5],
  ...[0, 1, 2, 3, 4, 5, 6, 7].map((index): [string, string, number] => [
    '--color-on-avatar',
    `--avatar-${String(index)}`,
    4.5,
  ]),
  // Value colors (FLD-044): the text of a value on its own tint.
  ...[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((index): [string, string, number] => [
    `--value-${String(index)}-fg`,
    `--value-${String(index)}-bg`,
    4.5,
  ]),
  // Card stripes (BRD-060): a value color as a graphic on the card, beside the name it stands for.
  ...[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((index): [string, string, number] => [
    `--value-${String(index)}-fg`,
    '--color-surface-raised',
    3,
  ]),
  // Code (CNT-045): its text, the language label and every highlight color on the code background.
  ...[
    '--code-text',
    '--color-text-muted',
    '--color-link',
    '--code-keyword',
    '--code-string',
    '--code-number',
    '--code-comment',
    '--code-title',
    '--code-type',
    '--code-attribute',
    '--code-meta',
  ].map((foreground): [string, string, number] => [foreground, '--code-bg', 4.5]),
];

afterEach(() => {
  setThemePreference('system');
  applyTheme('light');
});

describe('tokens', () => {
  for (const theme of ['light', 'dark'] as Theme[]) {
    it(`UI-032: text and controls have enough contrast in the ${theme} theme`, () => {
      applyTheme(theme);
      const failures = PAIRS.map(([fg, bg, min]) => ({ fg, bg, min, ratio: contrast(fg, bg) }))
        .filter(({ ratio, min }) => ratio < min)
        .map(({ fg, bg, ratio, min }) => `${fg} on ${bg}: ${ratio.toFixed(2)} < ${String(min)}`);
      expect(failures).toEqual([]);
    });
  }

  it('switches every surface with the theme', () => {
    applyTheme('light');
    const light = rgb(token('--color-canvas'));
    applyTheme('dark');
    const dark = rgb(token('--color-canvas'));
    expect(luminance(light)).toBeGreaterThan(0.9);
    expect(luminance(dark)).toBeLessThan(0.05);
    expect(getComputedStyle(document.documentElement).colorScheme).toBe('dark');
  });

  it('follows the chosen preference', () => {
    setThemePreference('dark');
    expect(getTheme()).toBe('dark');
    setThemePreference('light');
    expect(getTheme()).toBe('light');
  });

  it('D-019: controls are at least 44 px on touch screens', () => {
    const probe = document.createElement('div');
    probe.style.height = 'var(--control-sm)';
    document.body.append(probe);
    const height = probe.getBoundingClientRect().height;
    probe.remove();
    if (isPhone()) expect(height).toBeGreaterThanOrEqual(44);
    else expect(height).toBe(28);
  });
});
