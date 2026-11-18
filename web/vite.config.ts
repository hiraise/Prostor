import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { playwright } from '@vitest/browser-playwright';
import { defineConfig } from 'vitest/config';

// Address of the Rust server in development (`make dev` starts both).
const api = process.env.TASKMASTER_API ?? 'http://127.0.0.1:7700';

// Oldest supported browsers (UI-035): Baseline 2024. The two latest Chrome, Edge, Firefox and
// Opera releases are far newer; Safari 17.4 is the real floor. `shell/browser-support.ts` checks
// the same baseline at runtime.
const browserTargets = ['chrome123', 'edge123', 'firefox125', 'safari17.4'];

// Component tests run in a real browser; their file names say so.
const browserTests = ['src/**/*.browser.test.ts', 'src/**/*.browser.svelte.test.ts'];

export default defineConfig({
  plugins: [
    sveltekit({
      // Single-page app: the server answers every app route with this shell (crates/server/src/web.rs).
      adapter: adapter({ fallback: 'index.html' }),
      compilerOptions: { runes: true },
      // Content Security Policy (architecture.md §8): the shell's inline scripts are allowed by
      // their hashes, nothing else runs. User HTML is sanitized on the server as well; this is
      // the second wall. `frame-ancestors` cannot be set here: the server sends it as a header.
      csp: {
        mode: 'hash',
        directives: {
          'default-src': ['self'],
          // 'wasm-unsafe-eval' lets the query language's WebAssembly module compile (D-038); it
          // allows no eval of scripts.
          'script-src': ['self', 'wasm-unsafe-eval'],
          'style-src': ['self', 'unsafe-inline'],
          'img-src': ['self', 'data:', 'blob:', 'https:'],
          'media-src': ['self', 'blob:'],
          'font-src': ['self'],
          'connect-src': ['self'],
          'object-src': ['none'],
          'base-uri': ['self'],
          'form-action': ['self'],
        },
      },
    }),
  ],
  build: {
    target: browserTargets,
    cssTarget: browserTargets,
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: { '/api': { target: api, ws: true } },
    // The e2e run writes traces and downloads into .playwright/; watching them reloads every
    // open dev page mid-keystroke for minutes.
    watch: { ignored: ['**/.playwright/**'] },
  },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'unit',
          include: ['src/**/*.test.ts'],
          exclude: browserTests,
          environment: 'node',
        },
      },
      {
        extends: true,
        // Pre-bundled up front, so Vite does not reload the page in the middle of a run.
        optimizeDeps: { include: ['axe-core'] },
        // Shared cases the server's tests run too (markdown-view.browser.test.ts reads them).
        server: { fs: { allow: ['.', '../crates/markup/tests'] } },
        test: {
          name: 'browser',
          include: browserTests,
          // One retry absorbs a browser page closed under machine load (D-047); a test that
          // needs it is flaky and gets fixed — vitest reports such a test as flaky.
          retry: 1,
          setupFiles: ['src/test/browser-setup.ts'],
          browser: {
            enabled: true,
            headless: true,
            // Every component test runs on a desktop and on a phone (D-019).
            instances: [
              {
                browser: 'chromium',
                name: 'desktop',
                viewport: { width: 1280, height: 800 },
                provider: playwright(),
              },
              {
                browser: 'chromium',
                name: 'phone',
                viewport: { width: 390, height: 844 },
                provider: playwright({ contextOptions: { hasTouch: true, isMobile: true } }),
              },
            ],
          },
        },
      },
    ],
  },
});
