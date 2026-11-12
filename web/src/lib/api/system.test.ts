import { describe, expect, it } from 'vitest';

import { ApiError, createApiClient } from './client.ts';
import type { Bootstrap, Readiness, paths } from './schema.gen.ts';
import { fetchBootstrap, fetchReadiness } from './system.ts';

function respond(status: number, body: unknown) {
  return createApiClient<paths>({
    fetch: (input) => {
      const url = input instanceof Request ? input.url : input.toString();
      expect(url).toMatch(/^\/api\/v1\//);
      return Promise.resolve(new Response(JSON.stringify(body), { status }));
    },
  });
}

describe('fetchReadiness', () => {
  it('returns the status of a ready server', async () => {
    const ready: Readiness = { status: 'ready', version: '0.1.0' };
    await expect(fetchReadiness(respond(200, ready))).resolves.toEqual(ready);
  });

  it('returns the status of a starting server instead of failing', async () => {
    const starting: Readiness = { status: 'migrating', version: '0.1.0' };
    await expect(fetchReadiness(respond(503, starting))).resolves.toEqual(starting);
  });
});

describe('fetchBootstrap', () => {
  it('returns the configuration', async () => {
    const bootstrap: Bootstrap = {
      version: '0.1.0',
      base_url: null,
      setup_required: false,
      email_enabled: false,
      gravatar: false,
      default_date_format: 'locale',
      default_home: 'issues',
      work_schedule: { day_minutes: 480, week_days: 5 },
      limits: { upload_max_bytes: 10_485_760 },
    };
    await expect(fetchBootstrap(respond(200, bootstrap))).resolves.toEqual(bootstrap);
  });

  it('fails with the server error code', async () => {
    const failure = fetchBootstrap(respond(503, { code: 'unavailable', message: 'starting' }));
    await expect(failure).rejects.toBeInstanceOf(ApiError);
    await expect(failure).rejects.toMatchObject({ status: 503, body: { code: 'unavailable' } });
  });
});
