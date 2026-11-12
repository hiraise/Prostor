// A stand-in for `XMLHttpRequest` (uploads, `#lib/api/upload.ts`). `fakeXhr()` records requests
// and lets the test drive each one: progress, an answer, a network failure. `xhrThroughFetch()`
// sends every request on through `fetch`, so the routes of `fakeApi` answer uploads too. Undo with
// `vi.unstubAllGlobals()`.
import { vi } from 'vitest';

type Handler = ((event: ProgressEvent) => void) | null;

// Node has no `ProgressEvent`; unit tests run there.
function progressEvent(type: string, loaded = 0, total = 0): ProgressEvent {
  return Object.assign(new Event(type), { lengthComputable: total > 0, loaded, total });
}

export class FakeXhr {
  method = '';
  url = '';
  headers: Record<string, string> = {};
  body: unknown = null;
  status = 0;
  responseText = '';
  aborted = false;
  sent = false;
  readonly upload: { onprogress: Handler } = { onprogress: null };
  onload: Handler = null;
  onerror: Handler = null;
  onabort: Handler = null;
  private responseHeaders: Record<string, string> = {};

  constructor(private readonly onsend: (request: FakeXhr) => void) {}

  open(method: string, url: string): void {
    this.method = method.toUpperCase();
    this.url = url;
  }

  setRequestHeader(name: string, value: string): void {
    this.headers[name.toLowerCase()] = value;
  }

  getResponseHeader(name: string): string | null {
    return this.responseHeaders[name.toLowerCase()] ?? null;
  }

  send(body: unknown = null): void {
    this.body = body;
    this.sent = true;
    this.onsend(this);
  }

  abort(): void {
    if (this.aborted || this.status !== 0) return;
    this.aborted = true;
    this.onabort?.(progressEvent('abort'));
  }

  /** Reports bytes sent. */
  progress(loaded: number, total: number): void {
    this.upload.onprogress?.(progressEvent('progress', loaded, total));
  }

  /** Answers the request; `body` is sent as JSON. */
  respond(status: number, body?: unknown, headers: Record<string, string> = {}): void {
    this.status = status;
    this.responseText = body === undefined ? '' : JSON.stringify(body);
    this.responseHeaders = Object.fromEntries(
      Object.entries({ 'x-request-id': 'test-request', ...headers }).map(([key, value]) => [
        key.toLowerCase(),
        value,
      ]),
    );
    this.onload?.(progressEvent('load'));
  }

  /** The request does not reach the server. */
  fail(): void {
    this.onerror?.(progressEvent('error'));
  }
}

function install(onsend: (request: FakeXhr) => void): void {
  vi.stubGlobal(
    'XMLHttpRequest',
    class extends FakeXhr {
      constructor() {
        super(onsend);
      }
    },
  );
}

/** Installs the fake; the returned list fills with the requests as they are sent. */
export function fakeXhr(): FakeXhr[] {
  const requests: FakeXhr[] = [];
  install((request) => requests.push(request));
  return requests;
}

/**
 * Installs the fake so that each request goes on through `fetch` (stub it with `fakeApi`) after
 * reporting the whole body as sent.
 */
export function xhrThroughFetch(): void {
  install((request) => {
    const size = request.body instanceof Blob ? request.body.size : 0;
    request.progress(size, size);
    void fetch(request.url, {
      method: request.method,
      headers: request.headers,
      body: request.body as BodyInit | null,
    }).then(
      async (response) => {
        const text = await response.text();
        request.respond(
          response.status,
          text ? (JSON.parse(text) as unknown) : undefined,
          Object.fromEntries(response.headers.entries()),
        );
      },
      () => {
        request.fail();
      },
    );
  });
}
