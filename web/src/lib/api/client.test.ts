import { describe, expect, expectTypeOf, it, vi } from 'vitest';

import { ApiError, NetworkError, createApiClient, fillPath, queryString } from './client.ts';

// A contract shaped like the generated `paths`, with the cases the real API will have.
interface Project {
  id: number;
  name: string;
  version: number;
}
interface ErrorJson {
  code: 'not_found' | 'conflict';
  message: string;
}
interface TestPaths {
  '/projects': {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: {
      parameters: { query?: { limit?: number; cursor?: string | null; tag?: string[] } };
      responses: { 200: { content: { 'application/json': { items: Project[] } } } };
    };
    post: {
      parameters: { query?: never; path?: never };
      requestBody: { content: { 'application/json': { name: string } } };
      responses: {
        201: { content: { 'application/json': Project } };
        409: { content: { 'application/json': ErrorJson } };
      };
    };
    put?: never;
  };
  '/projects/{id}': {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: {
      parameters: { path: { id: number } };
      responses: {
        200: { content: { 'application/json': Project } };
        404: { content: { 'application/json': ErrorJson } };
      };
    };
    delete: {
      parameters: { path: { id: number } };
      responses: { 204: { content?: never } };
    };
  };
}

interface Recorded {
  url: string;
  method: string;
  headers: Headers;
  body: string | null;
}

function fakeServer(answer: (request: Recorded) => Response) {
  const calls: Recorded[] = [];
  const fetcher: typeof fetch = (input, init) => {
    const call: Recorded = {
      url: input instanceof Request ? input.url : input.toString(),
      method: init?.method ?? 'GET',
      headers: new Headers(init?.headers),
      body: typeof init?.body === 'string' ? init.body : null,
    };
    calls.push(call);
    return Promise.resolve(answer(call));
  };
  return { calls, fetcher };
}

const json = (status: number, body: unknown, headers: Record<string, string> = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json', ...headers },
  });

describe('API client', () => {
  it('fills path parameters and returns the typed body', async () => {
    const server = fakeServer(() => json(200, { id: 7, name: 'Core', version: 1 }));
    const api = createApiClient<TestPaths>({ fetch: server.fetcher });

    const project = await api.get('/projects/{id}', { path: { id: 7 } });

    expectTypeOf(project).toEqualTypeOf<Project>();
    expect(project.name).toBe('Core');
    expect(server.calls[0]).toMatchObject({ url: '/api/v1/projects/7', method: 'GET' });
    expect(server.calls[0]?.headers.get('accept')).toBe('application/json');
  });

  it('sends the query and leaves out absent values', async () => {
    const server = fakeServer(() => json(200, { items: [] }));
    const api = createApiClient<TestPaths>({ fetch: server.fetcher });

    await api.get('/projects', { query: { limit: 20, cursor: null, tag: ['a', 'b c'] } });
    await api.get('/projects');

    expect(server.calls.map((call) => call.url)).toEqual([
      '/api/v1/projects?limit=20&tag=a&tag=b+c',
      '/api/v1/projects',
    ]);
  });

  it('sends a JSON body', async () => {
    const server = fakeServer(() => json(201, { id: 1, name: 'New', version: 1 }));
    const api = createApiClient<TestPaths>({ fetch: server.fetcher });

    const created = await api.post('/projects', { body: { name: 'New' } });

    expectTypeOf(created).toEqualTypeOf<Project>();
    expect(server.calls[0]?.method).toBe('POST');
    expect(server.calls[0]?.headers.get('content-type')).toBe('application/json');
    expect(server.calls[0]?.body).toBe('{"name":"New"}');
  });

  it('returns nothing for 204', async () => {
    const server = fakeServer(() => new Response(null, { status: 204 }));
    const api = createApiClient<TestPaths>({ fetch: server.fetcher });

    await expect(api.delete('/projects/{id}', { path: { id: 3 } })).resolves.toBeUndefined();
  });

  it('throws the server error with its code and request ID', async () => {
    const server = fakeServer(() =>
      json(404, { code: 'not_found', message: 'no project' }, { 'x-request-id': 'req-1' }),
    );
    const onError = vi.fn();
    const api = createApiClient<TestPaths>({ fetch: server.fetcher, onError });

    const failure = api.get('/projects/{id}', { path: { id: 9 } });

    await expect(failure).rejects.toBeInstanceOf(ApiError);
    await expect(failure).rejects.toMatchObject({
      status: 404,
      code: 'not_found',
      requestId: 'req-1',
    });
    expect(onError).toHaveBeenCalledOnce();
  });

  it('derives a code when a proxy answers without our error body', async () => {
    const server = fakeServer(() => new Response('<html>Bad gateway</html>', { status: 502 }));
    const api = createApiClient<TestPaths>({ fetch: server.fetcher });

    await expect(api.get('/projects')).rejects.toMatchObject({
      status: 502,
      body: null,
      code: 'unavailable',
    });
  });

  it('returns an accepted error status as data, typed by the contract', async () => {
    const server = fakeServer(() => json(409, { code: 'conflict', message: 'taken' }));
    const api = createApiClient<TestPaths>({ fetch: server.fetcher });

    const answer = await api.post('/projects', { body: { name: 'Dup' }, accept: [409] });

    expectTypeOf(answer).toEqualTypeOf<Project | ErrorJson>();
    expect(answer).toEqual({ code: 'conflict', message: 'taken' });
  });

  it('reports a network failure as NetworkError, but not an abort', async () => {
    const onError = vi.fn();
    const offline = createApiClient<TestPaths>({
      fetch: () => Promise.reject(new TypeError('Failed to fetch')),
      onError,
    });
    await expect(offline.get('/projects')).rejects.toBeInstanceOf(NetworkError);
    expect(onError).toHaveBeenCalledOnce();

    const controller = new AbortController();
    controller.abort();
    const aborted = createApiClient<TestPaths>({
      fetch: () => Promise.reject(new DOMException('aborted', 'AbortError')),
      onError,
    });
    await expect(aborted.get('/projects', { signal: controller.signal })).rejects.toBeInstanceOf(
      DOMException,
    );
    expect(onError).toHaveBeenCalledOnce();
  });

  it('checks inputs against the contract at compile time', () => {
    const api = createApiClient<TestPaths>({ fetch: () => Promise.resolve(json(200, {})) });
    // Each line is a type error; the calls are never awaited, only type-checked.
    const unchecked = () => {
      // @ts-expect-error a path parameter is required
      void api.get('/projects/{id}');
      // @ts-expect-error the body has the wrong shape
      void api.post('/projects', { body: { title: 'x' } });
      // @ts-expect-error the path has no PUT
      void api.put('/projects', {});
      // @ts-expect-error unknown path
      void api.get('/nothing');
    };
    expect(typeof unchecked).toBe('function');
  });
});

describe('fillPath and queryString', () => {
  it('encodes path parameters', () => {
    expect(fillPath('/issues/{key}', { key: 'A B/1' })).toBe('/issues/A%20B%2F1');
    expect(() => fillPath('/issues/{key}', {})).toThrow(/key/);
  });

  it('builds query strings', () => {
    expect(queryString(undefined)).toBe('');
    expect(queryString({ a: undefined, b: null })).toBe('');
    expect(queryString({ q: 'state: open', all: true, n: 0 })).toBe(
      '?q=state%3A+open&all=true&n=0',
    );
  });
});
