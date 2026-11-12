// A stand-in for the server in component tests: answers `fetch` calls to `/api/v1` from a table of
// routes and records every call. Undo with `vi.unstubAllGlobals()`.
import { vi } from 'vitest';

export interface FakeRequest {
  method: string;
  /** Path after `/api/v1`, such as `/session`. */
  path: string;
  query: URLSearchParams;
  /** Parsed JSON, or the `Blob` of a raw body (an upload). */
  body: unknown;
}

export interface FakeAnswer {
  status?: number;
  body?: unknown;
}

export type FakeRoute = FakeAnswer | ((request: FakeRequest) => FakeAnswer | Promise<FakeAnswer>);

/** Installs the fake; routes are keyed `'POST /session'`. Unknown routes answer 404. */
export function fakeApi(routes: Record<string, FakeRoute>): FakeRequest[] {
  const calls: FakeRequest[] = [];
  vi.stubGlobal('fetch', async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = new URL(input instanceof Request ? input.url : String(input), location.href);
    const method = (init?.method ?? 'GET').toUpperCase();
    const text = typeof init?.body === 'string' ? init.body : null;
    const request: FakeRequest = {
      method,
      path: url.pathname.replace(/^\/api\/v1/, ''),
      query: url.searchParams,
      body:
        text !== null
          ? (JSON.parse(text) as unknown)
          : init?.body instanceof Blob
            ? init.body
            : undefined,
    };
    calls.push(request);
    const route = routes[`${method} ${request.path}`];
    const answer: FakeAnswer =
      typeof route === 'function'
        ? await route(request)
        : (route ?? { status: 404, body: { code: 'not_found', message: 'No such route' } });
    const status = answer.status ?? 200;
    return new Response(status === 204 ? null : JSON.stringify(answer.body ?? {}), {
      status,
      headers: { 'content-type': 'application/json', 'x-request-id': 'test-request' },
    });
  });
  return calls;
}

/** An error answer in the API's format. */
export function failure(status: number, code: string, fields?: unknown[]): FakeAnswer {
  return { status, body: { code, message: code, fields } };
}
