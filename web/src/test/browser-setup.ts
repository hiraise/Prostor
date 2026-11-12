// Setup of component tests (vite.config.ts → browser project): the app's global styles, so
// components render with real tokens, themes and focus styles.
import '#lib/theme/global.css';

// Pages follow live topics through the tab's connection; tests give their own, so the real socket
// never opens and never retries in the background.
class InertSocket extends EventTarget {
  readonly readyState = 0;
  onopen = null;
  onmessage = null;
  onclose = null;
  onerror = null;
  send(): void {
    // Never connected.
  }
  close(): void {
    // Never connected.
  }
}
globalThis.WebSocket = InertSocket as unknown as typeof WebSocket;
