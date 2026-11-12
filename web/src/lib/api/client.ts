// Typed client of `/api/v1` (D-006). Paths, parameters, bodies and responses are checked against the
// generated API types (`schema.gen.ts`, `make api`): a changed endpoint breaks the build, not the UI.
// Conventions it relies on: architecture.md §10.
import type { ErrorBody, ErrorCode, paths } from './schema.gen.ts';

/** Base path of the API; paths in the API document are relative to it. */
export const API_BASE = '/api/v1';

type Method = 'get' | 'post' | 'put' | 'patch' | 'delete';
type SuccessStatus = 200 | 201 | 202 | 204;

/** Paths of `Paths` that have method `M`. */
export type PathsWith<Paths, M extends Method> = {
  [P in keyof Paths]: Paths[P] extends Record<M, object> ? P : never;
}[keyof Paths] &
  string;

type Operation<Paths, P extends keyof Paths, M extends Method> =
  Paths[P] extends Record<M, infer Op> ? Op : never;

type Responses<Op> = Op extends { responses: infer R } ? R : never;
type JsonOf<R> = R extends { content: { 'application/json': infer T } } ? T : undefined;
type BodyAt<Op, S> = {
  [K in keyof Responses<Op>]: K extends S ? JsonOf<Responses<Op>[K]> : never;
}[keyof Responses<Op>];

/** Body of a successful answer; `undefined` for 204. */
export type Success<Op> = BodyAt<Op, SuccessStatus>;
/** Error statuses the operation documents. */
type ErrorStatus<Op> = Exclude<keyof Responses<Op>, SuccessStatus> & number;

type Parameters<Op> = Op extends { parameters: infer X } ? X : never;
type Absent<T> = [T] extends [never] ? true : [T] extends [undefined] ? true : false;
type PathParams<Op> = Parameters<Op> extends { path?: infer X } ? X : never;
type QueryParams<Op> = Parameters<Op> extends { query?: infer X } ? X : never;
type JsonBody<RB> = RB extends { content: { 'application/json': infer B } } ? B : never;
/** A required body, an optional one, or none (`requestBody?: never` in the generated types). */
type BodyField<Op> = Op extends { requestBody: infer RB }
  ? { body: JsonBody<RB> }
  : Op extends { requestBody?: infer RB }
    ? [NonNullable<RB>] extends [never]
      ? { body?: never }
      : { body?: JsonBody<NonNullable<RB>> }
    : { body?: never };

type Inputs<Op> = (Absent<PathParams<Op>> extends true
  ? { path?: never }
  : { path: PathParams<Op> }) &
  (Absent<QueryParams<Op>> extends true ? { query?: never } : { query?: QueryParams<Op> }) &
  BodyField<Op>;

/** Options of one call: typed inputs plus transport options. */
export type CallOptions<Op, S extends number = never> = Inputs<Op> & {
  signal?: AbortSignal;
  headers?: Record<string, string>;
  /** The `fetch` to use for this call, e.g. the one SvelteKit passes to `load`. */
  fetch?: typeof fetch;
  /** Error statuses whose documented body is returned instead of thrown. */
  accept?: readonly S[];
};

type RequiredKeys<T> = { [K in keyof T]-?: object extends Pick<T, K> ? never : K }[keyof T];
/** The options argument may be left out when nothing in it is required. */
type OptionsArg<Op, S extends number> = [RequiredKeys<Inputs<Op>>] extends [never]
  ? [options?: CallOptions<Op, S>]
  : [options: CallOptions<Op, S>];

type Call<Paths, M extends Method> = <
  P extends PathsWith<Paths, M>,
  S extends ErrorStatus<Operation<Paths, P, M>> = never,
>(
  path: P,
  ...options: OptionsArg<Operation<Paths, P, M>, S>
) => Promise<Success<Operation<Paths, P, M>> | BodyAt<Operation<Paths, P, M>, S>>;

export interface ApiClient<Paths> {
  get: Call<Paths, 'get'>;
  post: Call<Paths, 'post'>;
  put: Call<Paths, 'put'>;
  patch: Call<Paths, 'patch'>;
  delete: Call<Paths, 'delete'>;
}

/** The server answered with an error. `code` is the server's code, or one derived from the status. */
export class ApiError extends Error {
  readonly status: number;
  readonly body: ErrorBody | null;
  readonly code: ErrorCode;
  /** `x-request-id` of the answer: what a user can quote to an administrator. */
  readonly requestId: string | null;

  constructor(status: number, body: ErrorBody | null, requestId: string | null = null) {
    super(body ? `${body.code}: ${body.message}` : `HTTP ${String(status)}`);
    this.name = 'ApiError';
    this.status = status;
    this.body = body;
    this.code = body?.code ?? codeForStatus(status);
    this.requestId = requestId;
  }
}

/** The request did not reach the server or its answer was lost (offline, DNS, proxy reset). */
export class NetworkError extends Error {
  readonly code = 'network';

  constructor(cause: unknown) {
    super('The request did not reach the server', { cause });
    this.name = 'NetworkError';
  }
}

/** A code for answers without our error body, e.g. from a proxy in front of the server. */
export function codeForStatus(status: number): ErrorCode {
  switch (status) {
    case 400:
      return 'invalid_request';
    case 401:
      return 'unauthenticated';
    case 403:
      return 'forbidden';
    case 404:
      return 'not_found';
    case 405:
      return 'method_not_allowed';
    case 409:
      return 'conflict';
    case 413:
      return 'payload_too_large';
    case 415:
      return 'unsupported_media_type';
    case 429:
      return 'rate_limited';
    case 502:
    case 503:
    case 504:
      return 'unavailable';
    default:
      return 'internal';
  }
}

export function isErrorBody(value: unknown): value is ErrorBody {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as Record<string, unknown>).code === 'string' &&
    typeof (value as Record<string, unknown>).message === 'string'
  );
}

type Scalar = string | number | boolean;

/** Puts path parameters into `/projects/{id}`; a missing one is a programming error. */
export function fillPath(template: string, params: Record<string, Scalar> | undefined): string {
  return template.replace(/\{([^}]+)\}/g, (_, name: string) => {
    const value = params?.[name];
    if (value === undefined) throw new Error(`Missing path parameter "${name}" for ${template}`);
    return encodeURIComponent(String(value));
  });
}

/** Query string from parameters: absent and null values are left out, arrays repeat the key. */
export function queryString(params: Record<string, unknown> | undefined): string {
  if (!params) return '';
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    for (const item of Array.isArray(value) ? (value as unknown[]) : [value]) {
      if (item === undefined || item === null) continue;
      if (typeof item === 'string' || typeof item === 'number' || typeof item === 'boolean') {
        search.append(key, String(item));
      } else {
        throw new Error(`Query parameter "${key}" must be a string, number or boolean`);
      }
    }
  }
  const text = search.toString();
  return text ? `?${text}` : '';
}

export interface ClientOptions {
  base?: string;
  fetch?: typeof fetch;
  /** Called for every failed call before it is thrown, e.g. to send the user to sign-in on 401. */
  onError?: (error: ApiError | NetworkError) => void;
}

interface RawOptions {
  fetch?: typeof fetch;
  path?: Record<string, Scalar>;
  query?: Record<string, unknown>;
  body?: unknown;
  signal?: AbortSignal;
  headers?: Record<string, string>;
  accept?: readonly number[];
}

export function createApiClient<Paths>(options: ClientOptions = {}): ApiClient<Paths> {
  const base = options.base ?? API_BASE;

  async function request(method: Method, path: string, raw: RawOptions = {}): Promise<unknown> {
    const fetcher = raw.fetch ?? options.fetch ?? globalThis.fetch;
    const url = base + fillPath(path, raw.path) + queryString(raw.query);
    const headers = new Headers({ accept: 'application/json', ...raw.headers });
    let body: string | undefined;
    if (raw.body !== undefined) {
      headers.set('content-type', 'application/json');
      body = JSON.stringify(raw.body);
    }

    let response: Response;
    try {
      response = await fetcher(url, {
        method: method.toUpperCase(),
        headers,
        body,
        signal: raw.signal,
        credentials: 'same-origin',
      });
    } catch (cause) {
      // An aborted call is the caller's decision, not a failure to report.
      if (raw.signal?.aborted) throw cause;
      const error = new NetworkError(cause);
      options.onError?.(error);
      throw error;
    }

    const data: unknown =
      response.status === 204 ? undefined : await response.json().catch(() => undefined);
    if (response.ok || raw.accept?.includes(response.status)) return data;

    const error = new ApiError(
      response.status,
      isErrorBody(data) ? data : null,
      response.headers.get('x-request-id'),
    );
    options.onError?.(error);
    throw error;
  }

  const call =
    (method: Method) =>
    (path: string, ...rest: [RawOptions?]) =>
      request(method, path, rest[0]);

  // The typed facade over `request`: the casts are the one place where the generated types meet
  // the untyped transport.
  return {
    get: call('get') as Call<Paths, 'get'>,
    post: call('post') as Call<Paths, 'post'>,
    put: call('put') as Call<Paths, 'put'>,
    patch: call('patch') as Call<Paths, 'patch'>,
    delete: call('delete') as Call<Paths, 'delete'>,
  };
}

type ErrorListener = (error: ApiError | NetworkError) => void;
const errorListeners = new Set<ErrorListener>();

/**
 * Calls `listener` for every failed call of `api`, before the caller sees the error (an ended
 * session sends the user to sign-in). Returns a function that stops listening.
 */
export function onApiError(listener: ErrorListener): () => void {
  errorListeners.add(listener);
  return () => {
    errorListeners.delete(listener);
  };
}

/** Passes a failure of a request made outside `api` (an upload) to the listeners of `onApiError`. */
export function reportApiError(error: ApiError | NetworkError): void {
  for (const listener of errorListeners) listener(error);
}

/** The client of this app's server. */
export const api: ApiClient<paths> = createApiClient<paths>({
  onError: reportApiError,
});
