// The life of a new issue's draft (ISS-010, owner decision of 2026-10-24): every new issue is a
// server draft from the moment its form opens. The summary, the description and the text fields
// are saved a moment after typing stops and when the form closes; fields, tags and visibility are
// saved at once by their own controls, as on the issue page. Closing keeps a draft with content
// and deletes an empty one; a tab closed meanwhile sends the same with `keepalive`, and the
// server's sweep removes what a crash leaves behind.
import { API_BASE, ApiError, api } from '#lib/api/client.ts';
import { showRuleMessages } from '#lib/ui/rule-messages.ts';
import type { Issue, IssuePatch, NewIssueLink, Upload } from '#lib/api/schema.gen.ts';
import { uploadFile } from '#lib/api/upload.ts';
import { preferences } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { audienceInput } from '../actions.ts';
import { saveField } from '../fields/save.ts';
import { issueRow } from '../issues.ts';

/** Milliseconds after the last keystroke before the texts are saved. */
export const SAVE_DELAY = 600;

export type SaveStatus = 'idle' | 'saving' | 'saved' | 'failed';

/** What closing the form did with the draft. */
export type CloseResult = 'deleted' | 'kept' | 'none';

export interface Seed {
  project: number;
  summary?: string;
  description?: string;
  /** Links the draft starts with: a parent, or the issue it was started from (ISS-009). */
  links?: NewIssueLink[];
}

function isConflict(error: unknown): boolean {
  return error instanceof ApiError && error.code === 'conflict';
}

export class DraftSession {
  /** The draft being edited; `null` before it exists and after it is closed. */
  id = $state<number | null>(null);
  summary = $state('');
  description = $state('');
  /** Text fields by field ID, as typed. */
  texts = $state<Record<number, string>>({});
  status = $state<SaveStatus>('idle');
  /** Files attached from the editors in this session. */
  files = $state(0);
  /** Links of the draft, as its form shows them (LNK-007). */
  links = $state(0);

  #timer: ReturnType<typeof setTimeout> | undefined;
  /** The summary or description changed since the last save. */
  #textsDirty = false;
  /** Text fields changed since the last save. */
  // Not reactive: nothing renders from it.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  #fieldsDirty = new Set<number>();
  #saving: Promise<void> | null = null;
  #listening = false;

  /** The stored draft. Reactive. */
  get issue(): Issue | undefined {
    return this.id === null ? undefined : issueRow(this.id);
  }

  /** Creates a draft; the answer carries the project's defaults (FLD-055). */
  async start(seed: Seed): Promise<Issue> {
    const answer = await api.post('/issues', {
      body: {
        project: seed.project,
        draft: true,
        summary: seed.summary ?? '',
        description: seed.description ?? '',
        ...(seed.links?.length ? { links: seed.links } : {}),
      },
    });
    this.#adopt(answer.issue);
    this.links = seed.links?.length ?? 0;
    return answer.issue;
  }

  /** Continues an existing draft of the person's; `false` when there is none by that ID. */
  async open(id: number): Promise<boolean> {
    const answer = await api.get('/issues/{id}', { path: { id: String(id) }, accept: [404] });
    if (!('id' in answer) || !answer.draft) return false;
    this.#adopt(answer);
    return true;
  }

  #adopt(draft: Issue): void {
    store.entities.table('issue').upsert(draft, 'full');
    this.id = draft.id;
    this.summary = draft.summary;
    this.description = draft.description;
    this.texts = Object.fromEntries(
      draft.fields
        .filter((entry) => entry.field.kind === 'text')
        .map((entry) => [entry.field.id, entry.text ?? '']),
    );
    this.files = 0;
    this.links = 0;
    this.status = 'idle';
    this.#textsDirty = false;
    this.#fieldsDirty.clear();
    this.#listen(true);
  }

  /** The summary or description changed: save a moment later. */
  typed(): void {
    this.#textsDirty = true;
    this.#schedule();
  }

  /** A text field changed: save a moment later. */
  typedField(fieldId: number): void {
    this.#fieldsDirty.add(fieldId);
    this.#schedule();
  }

  #schedule(): void {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.flush().catch(() => undefined), SAVE_DELAY);
  }

  get pending(): boolean {
    return this.#textsDirty || this.#fieldsDirty.size > 0;
  }

  /** Saves what is typed but not saved yet. */
  async flush(): Promise<void> {
    clearTimeout(this.#timer);
    // One save at a time; changes typed meanwhile go in the next.
    while (this.#saving) await this.#saving;
    if (!this.pending || this.id === null) return;
    this.#saving = this.#save(this.id).finally(() => {
      this.#saving = null;
    });
    await this.#saving;
  }

  async #save(id: number): Promise<void> {
    this.status = 'saving';
    try {
      if (this.#textsDirty) {
        this.#textsDirty = false;
        const body: IssuePatch = { summary: this.summary, description: this.description };
        await this.#patch(id, body);
      }
      const fields = [...this.#fieldsDirty];
      this.#fieldsDirty.clear();
      for (const fieldId of fields) {
        const entry = issueRow(id)?.fields.find((other) => other.field.id === fieldId);
        if (!entry) continue;
        const text = this.texts[fieldId] ?? '';
        await saveField(id, { ...entry, text }, { kind: 'set', value: text === '' ? null : text });
      }
      this.status = this.pending ? 'saving' : 'saved';
    } catch (error) {
      this.status = 'failed';
      throw error;
    }
  }

  /**
   * Sends the texts on the draft's `text_version`. A conflict on a draft can only come from the
   * person's other tab: the texts here are the newer ones, so they are sent once more on the
   * version the server has.
   */
  async #patch(id: number, body: IssuePatch): Promise<void> {
    const send = async (version: number): Promise<Issue> => {
      const answer = await api.patch('/issues/{id}', {
        path: { id },
        body: { ...body, text_version: version },
      });
      return answer.issue;
    };
    let saved: Issue;
    try {
      saved = await send(issueRow(id)?.text_version ?? 0);
    } catch (error) {
      if (!isConflict(error)) throw error;
      const now = await api.get('/issues/{id}', { path: { id: String(id) } });
      saved = await send(now.text_version);
    }
    store.entities.table('issue').upsert(saved, 'full');
  }

  /** Nothing worth keeping: no texts, files, tags or links. */
  isEmpty(): boolean {
    const issue = this.issue;
    const texts = [this.summary, this.description, ...Object.values(this.texts)];
    return (
      texts.every((text) => text.trim() === '') &&
      this.files === 0 &&
      this.links === 0 &&
      (issue?.tags.length ?? 0) === 0
    );
  }

  /** Uploads a file from an editor and attaches it to the draft at once (ISS-007, CNT-034). */
  async upload(file: File): Promise<Upload> {
    const id = this.id;
    if (id === null) throw new Error('no draft');
    const staged = await uploadFile(file);
    const page = await api.post('/issues/{id}/attachments', {
      path: { id },
      body: { uploads: [staged.id] },
    });
    this.files += 1;
    // A name already on the draft gets a suffix: the text must name the attached file.
    const attached = page.attachments.at(-1);
    return attached ? { ...staged, name: attached.name } : staged;
  }

  /** Saves the visibility at once (ISS-041). */
  async setAudience(audience: Issue['audience']): Promise<void> {
    const id = this.id;
    if (id === null) return;
    await store.update('issue', id, { audience }, async () => {
      const answer = await api.patch('/issues/{id}', {
        path: { id },
        body: { audience: audienceInput(audience) },
      });
      return answer.issue;
    });
  }

  /**
   * The form closes: what is typed is saved, then an empty draft is deleted and any other kept.
   * The draft leaves the session either way.
   */
  async close(): Promise<CloseResult> {
    const id = this.id;
    if (id === null) return 'none';
    if (this.isEmpty()) {
      await this.discard();
      return 'deleted';
    }
    try {
      await this.flush();
    } finally {
      this.#detach();
    }
    return 'kept';
  }

  /** «Отменить и удалить». */
  async discard(): Promise<void> {
    const id = this.id;
    if (id === null) return;
    clearTimeout(this.#timer);
    this.#textsDirty = false;
    this.#fieldsDirty.clear();
    this.#detach();
    await this.#delete(id);
  }

  async #delete(id: number): Promise<void> {
    try {
      await api.delete('/issues/{id}', { path: { id } });
    } catch (error) {
      if (!(error instanceof ApiError && error.code === 'not_found')) throw error;
    }
    store.entities.table('issue').remove(id);
  }

  /** Publishes the draft (ISS-008); the person's last project is remembered (ISS-005). */
  async publish(): Promise<Issue> {
    await this.flush();
    const id = this.id;
    if (id === null) throw new Error('no draft');
    const answer = await api.post('/issues/{id}/publish', { path: { id } });
    showRuleMessages(answer.messages);
    store.entities.table('issue').upsert(answer.issue, 'full');
    this.#detach();
    preferences.set({ last_project: answer.issue.project.id });
    return answer.issue;
  }

  /** Lets the draft go without saving or deleting it: another form takes it over. */
  release(): void {
    this.#detach();
  }

  /** «Удалить все черновики» (ISS-011); this draft goes too. */
  async deleteAll(): Promise<void> {
    clearTimeout(this.#timer);
    this.#textsDirty = false;
    this.#fieldsDirty.clear();
    const id = this.id;
    this.#detach();
    await api.delete('/issues/drafts');
    if (id !== null) store.entities.table('issue').remove(id);
  }

  #detach(): void {
    this.#listen(false);
    this.id = null;
  }

  #listen(on: boolean): void {
    if (typeof window === 'undefined' || on === this.#listening) return;
    this.#listening = on;
    if (on) window.addEventListener('pagehide', this.#onPageHide);
    else window.removeEventListener('pagehide', this.#onPageHide);
  }

  /** The tab closes with the form open: the last save or the deletion goes with `keepalive`. */
  readonly #onPageHide = (): void => {
    const id = this.id;
    if (id === null) return;
    const url = `${API_BASE}/issues/${String(id)}`;
    const init: RequestInit = {
      keepalive: true,
      credentials: 'same-origin',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
    };
    if (this.isEmpty()) {
      void fetch(url, { ...init, method: 'DELETE' }).catch(() => undefined);
    } else if (this.#textsDirty) {
      const body: IssuePatch = {
        summary: this.summary,
        description: this.description,
        text_version: this.issue?.text_version ?? 0,
      };
      void fetch(url, { ...init, method: 'PATCH', body: JSON.stringify(body) }).catch(
        () => undefined,
      );
    }
  };
}
