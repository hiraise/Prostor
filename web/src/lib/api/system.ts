// Calls to the server's public endpoints: readiness and the configuration loaded before rendering.
import { api as defaultApi, type ApiClient } from './client.ts';
import type { Bootstrap, Readiness, paths } from './schema.gen.ts';

/** Load dependency of the configuration: `invalidate(BOOTSTRAP_DEPENDENCY)` loads it again. */
export const BOOTSTRAP_DEPENDENCY = 'app:bootstrap';

/** Whether the server is ready; a starting server answers 503 with the same shape. */
export function fetchReadiness(client: ApiClient<paths> = defaultApi): Promise<Readiness> {
  return client.get('/health/ready', { accept: [503] });
}

let latest: Bootstrap | null = null;

/** The configuration the app needs before it renders; the last one loaded is remembered. */
export async function fetchBootstrap(
  client: ApiClient<paths> = defaultApi,
  fetcher?: typeof fetch,
): Promise<Bootstrap> {
  const bootstrap = await client.get('/bootstrap', { fetch: fetcher });
  latest = bootstrap;
  return bootstrap;
}

/** The last configuration loaded; `null` before the first load. */
export function knownBootstrap(): Bootstrap | null {
  return latest;
}

/** Replaces the remembered configuration, for tests. */
export function setBootstrapForTests(bootstrap: Bootstrap | null): void {
  latest = bootstrap;
}
