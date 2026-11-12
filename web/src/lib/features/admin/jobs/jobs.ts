// The jobs of «Задания» (ADM-031, ADM-032; plan step 1.11 §6.2): the admin screen's calls and the
// Russian names of the machine kinds. The list is a plain request per page — it lives only while
// the screen is open.
import { api } from '#lib/api/client.ts';
import type { AdminJob, AdminJobState, AdminJobs } from '#lib/api/schema.gen.ts';
import { msg } from '#lib/i18n/message.ts';

/** What the list shows: only jobs in this state, and the words of the free-text filter. */
export interface JobsQuery {
  state?: AdminJobState;
  /** Words matched against the kind and the initiator's name; a leading `-` excludes. */
  q?: string;
  /** Only jobs with a smaller id: keyset pagination, newest first. */
  before?: number;
}

/** One page of jobs, newest first. Needs `system.read`. */
export function listJobs(query: JobsQuery = {}, signal?: AbortSignal): Promise<AdminJobs> {
  return api.get('/admin/jobs', {
    query: {
      state: query.state,
      q: query.q?.trim() === '' ? undefined : query.q?.trim(),
      before: query.before,
      limit: PAGE,
    },
    signal,
  });
}

/** How many rows one page holds; «Показать ещё» asks for the next one. */
export const PAGE = 30;

/**
 * Cancels a job (ADM-031): a queued one at once, a running one only when its kind may be; the
 * server refuses anything else with 409 and its message. Needs `system.write`.
 */
export function cancelJob(id: number): Promise<AdminJob> {
  return api.post('/admin/jobs/{id}/cancel', { path: { id } });
}

/**
 * Retries a failed job (ADM-032): a fresh one with the same kind and payload, the administrator
 * as its initiator. Only a failed job; the server refuses anything else with 409 and its message.
 * Needs `system.write`.
 */
export function retryJob(id: number): Promise<AdminJob> {
  return api.post('/admin/jobs/{id}/retry', { path: { id } });
}

// The kinds a person may meet, our names for the machine words; an unknown kind (a newer server)
// shows as the machine word itself.
export const KIND_NAMES: Record<string, { name: () => string }> = {
  'project.delete': { name: msg({ en: 'Project deletion', ru: 'Удаление проекта' }) },
  'issues.fields': {
    name: msg({ en: 'Field values in issues', ru: 'Значения полей в задачах' }),
  },
  'mail.send': { name: msg({ en: 'Email', ru: 'Письмо' }) },
  'content.render': { name: msg({ en: 'Text re-rendering', ru: 'Перерисовка текстов' }) },
  'files.remove': { name: msg({ en: 'File removal', ru: 'Удаление файлов' }) },
  'uploads.expire': { name: msg({ en: 'Upload expiry', ru: 'Истечение загрузок' }) },
  'attachments.inspect': { name: msg({ en: 'Picture inspection', ru: 'Просмотр картинок' }) },
};

/** The kind as the person reads it; an unknown one keeps its machine word. */
export function kindName(kind: string): string {
  return KIND_NAMES[kind]?.name() ?? kind;
}

/** Whether the row offers «Отменить»: queued always, running when its kind may be stopped. */
export function mayCancel(job: AdminJob): boolean {
  return job.state === 'queued' || (job.state === 'running' && job.cancellable);
}

/** Whether the row offers «Повторить»: only a failed job (ADM-032). */
export function mayRetry(job: AdminJob): boolean {
  return job.state === 'failed';
}
