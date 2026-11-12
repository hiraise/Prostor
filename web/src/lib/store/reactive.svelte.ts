// The few runes the data layer needs, so its classes stay plain TypeScript with ordinary maps for
// bookkeeping and only the state views read is reactive.

/** One reactive value: reading `value` inside a component or `$derived` tracks it. */
export class Cell<T> {
  value = $state() as T;

  constructor(initial: T) {
    this.value = initial;
  }
}

/** A deeply reactive proxy of a plain object; writes through it wake its readers. */
export function reactive<T extends object>(value: T): T {
  const proxy = $state(value);
  return proxy;
}

/** A plain deep copy of a value that may contain reactive proxies (which cannot be cloned). */
export function snapshot<T>(value: T): T {
  // `Snapshot<T>` is T without proxies; the data layer only stores plain JSON values.
  const copy: unknown = $state.snapshot(value);
  // The server build of Svelte (Node tests) hands plain values back uncopied.
  return (copy === value && typeof value === 'object' ? structuredClone(value) : copy) as T;
}
