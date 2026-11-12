// Reference counts with delayed eviction: what nobody retains is evicted after a delay, and
// retaining it again before then cancels the eviction. The app stays open all day, so data no view
// shows must not pile up, but a view opened again soon should still find its data.

export class Retention<K> {
  readonly #counts = new Map<K, number>();
  readonly #timers = new Map<K, ReturnType<typeof setTimeout>>();
  readonly #delayMs: number;
  readonly #evict: (key: K) => void;

  /** `delayMs` of `Infinity` never evicts. */
  constructor(delayMs: number, evict: (key: K) => void) {
    this.#delayMs = delayMs;
    this.#evict = evict;
  }

  /** Keeps `key` until the returned function is called (calling it again does nothing). */
  retain(key: K): () => void {
    this.#counts.set(key, (this.#counts.get(key) ?? 0) + 1);
    this.#cancel(key);
    let held = true;
    return () => {
      if (!held) return;
      held = false;
      const count = (this.#counts.get(key) ?? 1) - 1;
      if (count > 0) {
        this.#counts.set(key, count);
      } else {
        this.#counts.delete(key);
        this.track(key);
      }
    };
  }

  isRetained(key: K): boolean {
    return this.#counts.has(key);
  }

  /** Starts the countdown for a key nobody retains, e.g. data just loaded by a preload. */
  track(key: K): void {
    if (this.#counts.has(key) || this.#timers.has(key) || !Number.isFinite(this.#delayMs)) return;
    const timer = setTimeout(() => {
      this.#timers.delete(key);
      if (!this.#counts.has(key)) this.#evict(key);
    }, this.#delayMs);
    this.#timers.set(key, timer);
  }

  /** Stops the countdown of a key that is gone anyway; its holders keep their counts. */
  forget(key: K): void {
    this.#cancel(key);
  }

  #cancel(key: K): void {
    const timer = this.#timers.get(key);
    if (timer === undefined) return;
    clearTimeout(timer);
    this.#timers.delete(key);
  }
}
