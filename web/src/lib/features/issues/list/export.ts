// The selection's export (QRY-084, step 2.2 §9): the one answer that is a file, not JSON — a
// plain `fetch` of what `client.ts` would send, answered with the browser's own download and the
// two counts the toolbar reads. They differ when the file was cut by the export cap or by issues
// the viewer may not see, which is the toolbar's note to show.
import { API_BASE, ApiError, NetworkError, isErrorBody, reportApiError } from '#lib/api/client.ts';

/** The file formats the server writes (§9). */
export type ExportFormat = 'csv' | 'xlsx';

/** What one export runs on: the chosen issues, or the query's every match as the list runs it. */
export type ExportBody =
  | { ids: number[]; format: ExportFormat }
  | {
      query: {
        q?: string;
        project?: number;
        tag?: number;
        saved_search?: number;
        hide_resolved?: boolean;
      };
      format: ExportFormat;
    };

/** What the answer said besides the file: the rows in it, and the matches.
 * `null` when the headers are absent, so nothing is said of them. */
export interface Exported {
  readonly exported: number | null;
  readonly total: number | null;
}

/** Exports issues as a file download; the counts come back for the truncation note. */
export async function exportIssues(body: ExportBody): Promise<Exported> {
  let response: Response;
  try {
    response = await fetch(`${API_BASE}/issues/export`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', accept: 'application/octet-stream' },
      body: JSON.stringify(body),
      credentials: 'same-origin',
    });
  } catch (cause) {
    const error = new NetworkError(cause);
    reportApiError(error);
    throw error;
  }
  if (!response.ok) {
    const data: unknown = await response.json().catch(() => undefined);
    const error = new ApiError(
      response.status,
      isErrorBody(data) ? data : null,
      response.headers.get('x-request-id'),
    );
    reportApiError(error);
    throw error;
  }
  const name =
    /filename="([^"]+)"/.exec(response.headers.get('content-disposition') ?? '')?.[1] ??
    (body.format === 'csv' ? 'issues.csv' : 'issues.xlsx');
  // The browser's own download of the answer's bytes; the object URL lives long enough for it.
  const url = URL.createObjectURL(await response.blob());
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 10_000);
  const count = (header: string): number | null => {
    const text = response.headers.get(header);
    const parsed = text === null ? Number.NaN : Number(text);
    return Number.isFinite(parsed) ? parsed : null;
  };
  return { exported: count('x-exported-count'), total: count('x-total-count') };
}
