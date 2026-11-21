// Accessibility checks for component tests (UI-032): axe-core with the WCAG 2.2 AA rule sets.
import axe from 'axe-core';
import { expect } from 'vitest';

/** Waits out finite animations (a dialog fading or scaling in measures differently for a
 *  moment — its panels open from `scale(0.98)`). Endless animations (spinners) are not
 *  waited for. */
async function settled(context: Element): Promise<void> {
  await Promise.all(
    context
      .getAnimations({ subtree: true })
      .filter((animation) => animation.effect?.getComputedTiming().endTime !== Infinity)
      .map((animation) => animation.finished.catch(() => undefined)),
  );
}

export async function expectAccessible(context: Element = document.body): Promise<void> {
  // Colors, too, are measured on the final frame.
  await settled(context);
  const result = await axe.run(context, {
    runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'] },
  });
  const problems = result.violations.map(
    (violation) =>
      `${violation.id}: ${violation.help} — ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`,
  );
  expect(problems).toEqual([]);
}

/** True in the phone profile of the browser tests (vite.config.ts). */
export function isPhone(): boolean {
  return matchMedia('(pointer: coarse)').matches;
}

/** Every visible button and link in `root` is at least `min` CSS pixels in both directions,
 *  measured after opening animations finish: a slow machine still scaling a dialog in would
 *  read a 44 px target as 43.1 px. */
export async function smallTargets(root: Element, min = 44): Promise<string[]> {
  await settled(root);
  return [...root.querySelectorAll<HTMLElement>('button, a[href], [role="menuitem"]')]
    .filter((element) => element.checkVisibility())
    .filter((element) => {
      const box = element.getBoundingClientRect();
      return box.width < min - 0.5 || box.height < min - 0.5;
    })
    .map((element) => element.getAttribute('aria-label') ?? element.textContent.trim());
}
