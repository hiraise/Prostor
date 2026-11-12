import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { fakeXhr } from '../../test/fake-xhr.ts';
import { ApiError, NetworkError, onApiError } from './client.ts';
import type { Bootstrap, Upload } from './schema.gen.ts';
import { setBootstrapForTests } from './system.ts';
import { uploadFile } from './upload.ts';

const BOOTSTRAP: Bootstrap = {
  version: '0.1.0',
  base_url: null,
  setup_required: false,
  email_enabled: false,
  gravatar: false,
  default_date_format: 'locale',
  default_home: 'issues',
  work_schedule: { day_minutes: 480, week_days: 5 },
  limits: { upload_max_bytes: 10 },
};

const UPLOAD: Upload = {
  id: 7,
  name: 'notes v2.txt',
  size: 5,
  content_type: 'text/plain',
  kind: 'other',
  url: '/api/v1/uploads/7/notes%20v2.txt',
  created_at: '2026-10-22T09:00:00Z',
};

function file(size: number, name = 'notes v2.txt'): File {
  return new File(['x'.repeat(size)], name, { type: 'text/plain' });
}

beforeEach(() => {
  setBootstrapForTests(BOOTSTRAP);
});

afterEach(() => {
  vi.unstubAllGlobals();
  setBootstrapForTests(null);
});

describe('uploadFile', () => {
  it('CNT-019: sends the raw file with its name and reports progress', async () => {
    const requests = fakeXhr();
    const progress = vi.fn();
    const sending = uploadFile(file(5), { onprogress: progress });
    const [request] = requests;
    expect(request?.method).toBe('POST');
    expect(request?.url).toBe('/api/v1/uploads?name=notes+v2.txt');
    expect(request?.headers['content-type']).toBe('application/octet-stream');
    expect(request?.body).toBeInstanceOf(File);
    request?.progress(2, 5);
    request?.progress(5, 5);
    request?.respond(201, UPLOAD);
    await expect(sending).resolves.toEqual(UPLOAD);
    expect(progress.mock.calls).toEqual([
      [2, 5],
      [5, 5],
    ]);
  });

  it('CNT-019: refuses a file over the limit without sending it', async () => {
    const requests = fakeXhr();
    const sending = uploadFile(file(11));
    await expect(sending).rejects.toBeInstanceOf(ApiError);
    await expect(sending).rejects.toMatchObject({ status: 413, code: 'payload_too_large' });
    expect(requests).toEqual([]);
  });

  it('CNT-019: sends any size while the limit is unknown', async () => {
    setBootstrapForTests(null);
    const requests = fakeXhr();
    const sending = uploadFile(file(11));
    requests[0]?.respond(201, UPLOAD);
    await expect(sending).resolves.toEqual(UPLOAD);
  });

  it('CNT-019: fails with the server error and reports it', async () => {
    const requests = fakeXhr();
    const reported = vi.fn();
    const stop = onApiError(reported);
    const sending = uploadFile(file(5));
    requests[0]?.respond(409, { code: 'conflict', message: 'Too many staged files' });
    await expect(sending).rejects.toMatchObject({
      status: 409,
      code: 'conflict',
      requestId: 'test-request',
    });
    expect(reported).toHaveBeenCalledOnce();
    stop();
  });

  it('CNT-019: derives the code of an answer without our body', async () => {
    const requests = fakeXhr();
    const sending = uploadFile(file(5));
    requests[0]?.respond(413, undefined);
    await expect(sending).rejects.toMatchObject({ status: 413, code: 'payload_too_large' });
  });

  it('CNT-019: a lost connection is a network error', async () => {
    const requests = fakeXhr();
    const sending = uploadFile(file(5));
    requests[0]?.fail();
    await expect(sending).rejects.toBeInstanceOf(NetworkError);
  });

  it('CNT-019: aborting stops the request with the signal reason', async () => {
    const requests = fakeXhr();
    const controller = new AbortController();
    const reported = vi.fn();
    const stop = onApiError(reported);
    const sending = uploadFile(file(5), { signal: controller.signal });
    controller.abort();
    await expect(sending).rejects.toMatchObject({ name: 'AbortError' });
    expect(requests[0]?.aborted).toBe(true);
    expect(reported).not.toHaveBeenCalled();
    stop();
  });

  it('CNT-019: an already aborted signal sends nothing', async () => {
    const requests = fakeXhr();
    const controller = new AbortController();
    controller.abort();
    await expect(uploadFile(file(5), { signal: controller.signal })).rejects.toMatchObject({
      name: 'AbortError',
    });
    expect(requests).toEqual([]);
  });
});
