// Sending files (CNT-019): the one request with a raw body. It goes over `XMLHttpRequest`, since
// `fetch` cannot report upload progress, and fails the way `client.ts` calls do: `ApiError` with
// the server's code, `NetworkError` when the request never arrived, the signal's reason when the
// caller aborts it. A file over the server's limit is refused before a byte is sent.
import {
  API_BASE,
  ApiError,
  NetworkError,
  api,
  isErrorBody,
  queryString,
  reportApiError,
} from './client.ts';
import type { Upload } from './schema.gen.ts';
import { knownBootstrap } from './system.ts';

export interface UploadOptions {
  signal?: AbortSignal;
  /** Bytes sent so far and in all; called as the file goes out. */
  onprogress?: (loaded: number, total: number) => void;
}

/** The largest file the server takes, if the configuration is loaded. */
export function uploadLimit(): number | null {
  return knownBootstrap()?.limits.upload_max_bytes ?? null;
}

/** Sends a file to the server, which keeps it staged until it is attached or used as a picture. */
export function uploadFile(file: File, options: UploadOptions = {}): Promise<Upload> {
  const { signal, onprogress } = options;
  const limit = uploadLimit();
  if (limit !== null && file.size > limit) {
    return Promise.reject(
      new ApiError(413, {
        code: 'payload_too_large',
        message: `The file has ${String(file.size)} bytes; at most ${String(limit)} are taken`,
      }),
    );
  }
  if (signal?.aborted) return Promise.reject(signal.reason as Error);

  return new Promise<Upload>((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    const stop = (): void => {
      xhr.abort();
    };

    function settle(): void {
      signal?.removeEventListener('abort', stop);
    }

    function fail(error: ApiError | NetworkError): void {
      reportApiError(error);
      reject(error);
    }

    xhr.open('POST', `${API_BASE}/uploads${queryString({ name: file.name })}`);
    xhr.setRequestHeader('accept', 'application/json');
    xhr.setRequestHeader('content-type', 'application/octet-stream');
    xhr.upload.onprogress = (event: ProgressEvent) => {
      onprogress?.(event.loaded, event.lengthComputable ? event.total : file.size);
    };
    xhr.onload = () => {
      settle();
      let data: unknown;
      try {
        data = JSON.parse(xhr.responseText) as unknown;
      } catch {
        data = undefined;
      }
      if (xhr.status >= 200 && xhr.status < 300 && data !== undefined) {
        resolve(data as Upload);
        return;
      }
      fail(
        new ApiError(
          xhr.status,
          isErrorBody(data) ? data : null,
          xhr.getResponseHeader('x-request-id'),
        ),
      );
    };
    xhr.onerror = () => {
      settle();
      fail(new NetworkError(new Error('The upload did not reach the server')));
    };
    xhr.onabort = () => {
      settle();
      // An aborted upload is the caller's decision, not a failure to report.
      reject(
        signal?.aborted
          ? (signal.reason as Error)
          : new DOMException('The upload was aborted', 'AbortError'),
      );
    };
    signal?.addEventListener('abort', stop, { once: true });
    xhr.send(file);
  });
}

/** Removes a staged upload that will not be used. */
export async function discardUpload(id: number): Promise<void> {
  await api.delete('/uploads/{id}', { path: { id } });
}

/** Picture types the server turns into avatars and logos (the `accept` of their file pickers). */
export const PICTURE_TYPES = 'image/png,image/jpeg,image/gif,image/webp';
