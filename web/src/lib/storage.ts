// Small values kept in this browser (local storage). Storage can be missing or blocked (private
// windows, strict settings): reads then return null and writes do nothing, so the app still works.

export function readStored(key: string): string | null {
  try {
    return globalThis.localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeStored(key: string, value: string | null): void {
  try {
    if (value === null) globalThis.localStorage.removeItem(key);
    else globalThis.localStorage.setItem(key, value);
  } catch {
    // Not stored: the value lasts until the tab is closed.
  }
}

/** Calls `listener` when another tab changes `key`; returns a function that stops listening. */
export function onStoredChange(key: string, listener: (value: string | null) => void): () => void {
  if (typeof window === 'undefined') return () => undefined;
  const handle = (event: StorageEvent): void => {
    if (event.key === key) listener(event.newValue);
  };
  window.addEventListener('storage', handle);
  return () => {
    window.removeEventListener('storage', handle);
  };
}
