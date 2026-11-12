// One clock for every relative phrase on the page ("5 minutes ago"): a single timer that ticks at
// each full minute while something shows a relative time, and stops when nothing does.
import { createSubscriber } from 'svelte/reactivity';

const MINUTE_MS = 60_000;

const subscribe = createSubscriber((update) => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const schedule = (): void => {
    clearTimeout(timer);
    timer = setTimeout(
      () => {
        update();
        schedule();
      },
      MINUTE_MS - (Date.now() % MINUTE_MS),
    );
  };
  // Timers sleep in background tabs; catch up as soon as the page is visible again.
  const onVisibility = (): void => {
    if (document.visibilityState !== 'visible') return;
    update();
    schedule();
  };
  schedule();
  if (typeof document !== 'undefined') document.addEventListener('visibilitychange', onVisibility);
  return () => {
    clearTimeout(timer);
    if (typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', onVisibility);
    }
  };
});

export const clock = {
  /** The current time; reading it inside a component or effect re-runs it every minute. */
  get now(): number {
    subscribe();
    return Date.now();
  },
};
