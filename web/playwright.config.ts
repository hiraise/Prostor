// End-to-end tests (conventions.md §4): the release flow of the app against a real server and a
// fresh database. `make e2e` builds both and prepares the database; then this config starts the
// server. The setup project runs the first-run wizard once; the flows then run on a desktop and on
// a phone (D-019).
import { defineConfig, devices } from '@playwright/test';

import { SETUP_CODE } from './e2e/accounts.ts';

const port = Number(process.env.PROSTOR_E2E_PORT ?? 7701);
const baseURL = `http://127.0.0.1:${String(port)}`;
const database = process.env.PROSTOR_E2E_DATABASE_URL;
if (!database)
  throw new Error('PROSTOR_E2E_DATABASE_URL is not set: run the tests with `make e2e`');

export default defineConfig({
  testDir: 'e2e',
  testMatch: '*.e2e.ts',
  outputDir: '.playwright/results',
  fullyParallel: false,
  forbidOnly: true,
  // One file at a time: with parallel workers the full suite fails one rotating spec per run
  // (issue.e2e's delete dialog never opened; every spec passes alone — backlog, 2026-11-07);
  // serial files removed the flake across runs. One retry stays for residual noise: two fails
  // still fail.
  workers: 1,
  retries: 1,
  reporter: [['list']],
  use: { baseURL, trace: 'retain-on-failure' },
  projects: [
    { name: 'setup', testMatch: 'setup.e2e.ts', use: { ...devices['Desktop Chrome'] } },
    {
      name: 'desktop',
      testIgnore: 'setup.e2e.ts',
      dependencies: ['setup'],
      use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 800 } },
    },
    {
      name: 'phone',
      testIgnore: 'setup.e2e.ts',
      dependencies: ['setup'],
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 390, height: 844 },
        hasTouch: true,
        isMobile: true,
      },
    },
  ],
  webServer: {
    command: '../target/debug/prostor',
    url: `${baseURL}/api/v1/health/ready`,
    reuseExistingServer: false,
    timeout: 60_000,
    stdout: 'ignore',
    stderr: 'pipe',
    env: {
      DATABASE_URL: database,
      PROSTOR_LISTEN: `127.0.0.1:${String(port)}`,
      PROSTOR_SETUP_CODE: SETUP_CODE,
      // Account emails go to Mailpit of `make db-up`; the tests read them from its API.
      PROSTOR_SMTP_URL: 'smtp://127.0.0.1:1025',
      PROSTOR_MAIL_FROM: 'Prostor <e2e@prostor.test>',
      // The instance key of the run: the mail screen saves its secrets sealed (NTF-004).
      PROSTOR_SECRET_KEY: 'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=',
      // Uploaded files of the run; `make e2e` empties the directory first. No Gravatar: the
      // tests stay offline.
      PROSTOR_FILES_DIR: '.playwright/files',
      PROSTOR_GRAVATAR_URL: '',
      RUST_LOG: 'warn',
    },
  },
});
