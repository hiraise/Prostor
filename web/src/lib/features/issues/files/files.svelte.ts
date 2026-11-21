// The files of the open issue (CNT-017, CNT-020, CNT-022, CNT-024): the list the attachments
// section shows, uploads in progress, and the file open in the lightbox. One per issue page, shared
// through the context with comments and editors, so a file attached anywhere on the page shows in
// the section. Rows live in the store (`attachment` kind); the list keeps their order.
import { getContext, setContext } from 'svelte';
import { SvelteSet } from 'svelte/reactivity';

import { api } from '#lib/api/client.ts';
import type { Attachment, AttachmentPage, Audience, Upload } from '#lib/api/schema.gen.ts';
import { uploadFile, type UploadOptions } from '#lib/api/upload.ts';
import { getLocale } from '#lib/i18n/locale.svelte.ts';
import { store } from '#lib/store/index.ts';
import { describeError } from '#lib/ui/toast.svelte.ts';

import { audienceInput } from '../actions.ts';

/** A dialog a file's menu asks for (AttachmentDialogs). */
export interface FileAction {
  kind: 'details' | 'rename' | 'visibility' | 'delete';
  file: Attachment;
}

export type SendFile = (file: File, options?: UploadOptions) => Promise<Upload>;

/** A file on its way: progress while it goes out, the reason if it failed. */
export interface PendingFile {
  key: number;
  name: string;
  loaded: number;
  total: number;
  error: string | null;
}

/** How a file opens: in the lightbox, in a new tab (PDF), or as a download. */
export function opensIn(file: Pick<Attachment, 'kind'>): 'lightbox' | 'tab' | 'download' {
  if (file.kind === 'image' || file.kind === 'video') return 'lightbox';
  return file.kind === 'pdf' ? 'tab' : 'download';
}

/** Bytes as people read them: «12 КБ», «3,4 МБ». */
export function fileSize(bytes: number): string {
  const units = ['byte', 'kilobyte', 'megabyte', 'gigabyte'] as const;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return new Intl.NumberFormat(getLocale(), {
    style: 'unit',
    unit: units[unit],
    unitDisplay: unit === 0 ? 'long' : 'short',
    maximumFractionDigits: value < 10 && unit > 0 ? 1 : 0,
  }).format(value);
}

export class IssueFiles {
  readonly issue: number;
  #list = $state.raw<Attachment[]>([]);
  #next = $state<number | null>(null);
  readonly #removed = new SvelteSet<number>();
  #send: SendFile;
  #keys = 0;
  pending = $state<PendingFile[]>([]);
  /** The file shown in the lightbox. */
  previewing = $state<number | null>(null);
  loadingMore = $state(false);

  constructor(issue: number, first: AttachmentPage, send: SendFile = uploadFile) {
    this.issue = issue;
    this.#list = first.attachments;
    this.#next = first.next;
    this.#send = send;
  }

  /** The issue's files in upload order, as the store holds them; those of deleted comments left out. */
  get items(): Attachment[] {
    const files = store.entities.table('attachment');
    const comments = store.entities.table('comment');
    return this.#list
      .filter((file) => !this.#removed.has(file.id))
      .map((file) => files.get(file.id) ?? file)
      .filter((file) => file.comment === null || !comments.get(file.comment)?.deleted);
  }

  get hasMore(): boolean {
    return this.#next !== null;
  }

  /** Whether the file was deleted on this page. */
  isRemoved(id: number): boolean {
    return this.#removed.has(id);
  }

  /** Files attached elsewhere on the page (a comment's) join the list. */
  add(files: Attachment[]): void {
    if (files.length === 0) return;
    store.put('attachment', files, 'full');
    const known = this.#list.map((file) => file.id);
    this.#list = [...this.#list, ...files.filter((file) => !known.includes(file.id))];
  }

  /** Loads the first page again (after missed live changes). */
  async reload(): Promise<void> {
    const page = await api.get('/issues/{id}/attachments', { path: { id: this.issue } });
    store.put('attachment', page.attachments, 'full');
    this.#list = page.attachments;
    this.#next = page.next;
    this.#removed.clear();
  }

  /** A file deleted elsewhere leaves the list (ISS-048). */
  forget(id: number): void {
    this.#removed.add(id);
  }

  /** A file attached elsewhere joins the list once loaded (ISS-048). */
  async fetchOne(id: number): Promise<void> {
    if (this.#list.some((file) => file.id === id)) return;
    const file = (await store.ensureEntity('attachment', id, { form: 'full' })).current;
    if (file) this.add([file]);
  }

  async more(): Promise<void> {
    if (this.#next === null || this.loadingMore) return;
    this.loadingMore = true;
    try {
      const page = await api.get('/issues/{id}/attachments', {
        path: { id: this.issue },
        query: { cursor: this.#next },
      });
      this.#next = page.next;
      this.add(page.attachments);
    } finally {
      this.loadingMore = false;
    }
  }

  /**
   * Uploads files and attaches them to the issue in one request (CNT-017); each file shows its
   * progress, and a failed one stays in `pending` with the reason. Returns what was attached.
   */
  async attach(files: File[]): Promise<Attachment[]> {
    if (files.length === 0) return [];
    const entries = files.map((file) => {
      this.#keys += 1;
      return { key: this.#keys, name: file.name, loaded: 0, total: file.size, error: null };
    });
    this.pending = [...this.pending, ...entries];
    const results = await Promise.allSettled(
      files.map((file, index) =>
        this.#send(file, {
          onprogress: (loaded, total) => {
            this.#progress(entries[index]?.key, loaded, total);
          },
        }),
      ),
    );
    const staged: Upload[] = [];
    results.forEach((result, index) => {
      const key = entries[index]?.key;
      if (result.status === 'fulfilled') staged.push(result.value);
      else this.#update(key, { error: describeError(result.reason).message });
    });
    const sent = results.flatMap((result, index) =>
      result.status === 'fulfilled' ? [entries[index]?.key] : [],
    );
    if (staged.length === 0) return [];
    try {
      const page = await api.post('/issues/{id}/attachments', {
        path: { id: this.issue },
        body: { uploads: staged.map((upload) => upload.id) },
      });
      this.add(page.attachments);
      this.pending = this.pending.filter((entry) => !sent.includes(entry.key));
      return page.attachments;
    } catch (error) {
      const reason = describeError(error).message;
      for (const key of sent) this.#update(key, { error: reason });
      throw error;
    }
  }

  /**
   * For an editor of the issue's own texts (edit mode): the file is attached at once and the
   * editor names it as the issue has it (a taken name gets « (2)»).
   */
  readonly uploadForText = async (file: File): Promise<Upload> => {
    const [attached] = await this.attach([file]);
    if (!attached) throw new Error(`${file.name} was not attached`);
    return {
      id: attached.id,
      name: attached.name,
      kind: attached.kind,
      content_type: attached.content_type,
      size: attached.size,
      url: attached.url,
      created_at: attached.created_at,
    };
  };

  /** Removes a failed upload from the list. */
  dismiss(key: number): void {
    this.pending = this.pending.filter((entry) => entry.key !== key);
  }

  rename(id: number, name: string): Promise<Attachment> {
    return this.#patch(id, { name }, { name });
  }

  setAudience(id: number, audience: Audience): Promise<Attachment> {
    return this.#patch(id, { audience }, { audience: audienceInput(audience) });
  }

  /** Deletes a file for good (CNT-024); texts naming it show it as missing. */
  async remove(id: number): Promise<void> {
    await api.delete('/attachments/{id}', { path: { id } });
    this.#removed.add(id);
    if (this.previewing === id) this.previewing = null;
    store.entities.table('attachment').remove(id);
    this.#refreshTexts();
  }

  async #patch(
    id: number,
    changes: Partial<Attachment>,
    body: { name?: string; audience?: { users: number[]; groups: number[] } },
  ): Promise<Attachment> {
    const saved = await store.update('attachment', id, changes, ({ version }) =>
      api.patch('/attachments/{id}', { path: { id }, body: { ...body, version } }),
    );
    if (changes.name !== undefined) this.#refreshTexts();
    return saved;
  }

  /** The issue's texts resolve file names on the server: ask again after a rename or delete. */
  #refreshTexts(): void {
    store.refetch('issue', [this.issue]).catch(() => undefined);
  }

  #update(key: number | undefined, changes: Partial<PendingFile>): void {
    this.pending = this.pending.map((entry) =>
      entry.key === key ? { ...entry, ...changes } : entry,
    );
  }

  /** Progress arrives per network flush — dozens of times a second per file; the bar shows
   * whole percents, so the state (and with it the list) changes only when one does. */
  #progress(key: number | undefined, loaded: number, total: number): void {
    const entry = key === undefined ? undefined : this.pending.find((one) => one.key === key);
    if (!entry) return;
    const percent = (at: number, of: number): number => (of > 0 ? Math.floor((100 * at) / of) : 0);
    if (percent(entry.loaded, entry.total) === percent(loaded, total)) return;
    this.#update(key, { loaded, total });
  }
}

const KEY = Symbol('issue-files');

/** Shares the page's files with what it contains (comments, editors). */
export function setIssueFiles(files: () => IssueFiles | null): void {
  setContext(KEY, files);
}

/** The page's files, if inside an issue page. */
export function issueFiles(): (() => IssueFiles | null) | undefined {
  return getContext<(() => IssueFiles | null) | undefined>(KEY);
}
