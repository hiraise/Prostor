// The fields of one project as its fields page shows them (FLD-051…FLD-060). A project's settings
// of a field have no ID of their own and only this page needs them, so they are not in the store:
// the page keeps the server's latest answer here and replaces it with each answer of a change.
import type { AttachField, ProjectField, ProjectFieldPatch } from '#lib/api/schema.gen.ts';

import {
  attachField,
  detachField,
  fetchProjectFields,
  forkFieldSet,
  moveProjectField,
  reorder,
  updateProjectField,
} from './fields.ts';

/** Settings a person changes in a project (FLD-055, FLD-056, FLD-060). */
export type ProjectFieldSettings = Omit<ProjectFieldPatch, 'version'>;

export class ProjectFieldList {
  readonly projectId: number;
  /** The fields in the project's order. */
  items = $state.raw<ProjectField[]>([]);
  /** The first load finished. */
  loaded = $state(false);
  loading = $state(false);
  /** Error of the last load; the fields from before stay. */
  error = $state<unknown>(null);
  #run = 0;

  constructor(projectId: number) {
    this.projectId = projectId;
  }

  /** Loads the fields (again); a slower earlier load never overwrites a later one. */
  async load(): Promise<void> {
    const run = ++this.#run;
    this.loading = true;
    try {
      const answer = await fetchProjectFields(this.projectId);
      if (run !== this.#run) return;
      this.items = answer.items;
      this.error = null;
      this.loaded = true;
    } catch (error) {
      if (run === this.#run) this.error = error;
      throw error;
    } finally {
      if (run === this.#run) this.loading = false;
    }
  }

  /** Loads again without throwing: a failure stays in `error`. */
  refresh(): void {
    this.load().catch(() => undefined);
  }

  find(fieldId: number): ProjectField | undefined {
    return this.items.find((item) => item.field.id === fieldId);
  }

  #replace(saved: ProjectField): ProjectField {
    this.items = this.items.map((item) => (item.field.id === saved.field.id ? saved : item));
    return saved;
  }

  /** Attaches a field at the end of the order (FLD-052, FLD-053). */
  async attach(input: AttachField): Promise<ProjectField> {
    const attached = await attachField(this.projectId, input);
    this.#run++;
    this.items = [...this.items.filter((item) => item.field.id !== attached.field.id), attached];
    return attached;
  }

  /** Saves settings on the version shown; a stale one is refused with `conflict`. */
  async save(fieldId: number, settings: ProjectFieldSettings): Promise<ProjectField> {
    const current = this.find(fieldId);
    if (!current) throw new Error(`Field ${String(fieldId)} is not in the project`);
    const saved = await updateProjectField(this.projectId, fieldId, {
      ...settings,
      version: current.version,
    });
    this.#run++;
    return this.#replace(saved);
  }

  /** Moves a field right after another (`null`: first); the order shows at once (FLD-058). */
  async move(fieldId: number, after: number | null): Promise<void> {
    const before = this.items;
    this.#run++;
    this.items = reorder(before, (item) => item.field.id, fieldId, after);
    try {
      const answer = await moveProjectField(this.projectId, fieldId, after);
      this.items = answer.items;
    } catch (error) {
      this.items = before;
      throw error;
    }
  }

  /** Detaches a field (FLD-054). */
  async detach(fieldId: number): Promise<void> {
    await detachField(this.projectId, fieldId);
    this.#run++;
    this.items = this.items.filter((item) => item.field.id !== fieldId);
  }

  /** Gives the project its own copy of the field's shared set (FLD-030). */
  async forkSet(fieldId: number): Promise<ProjectField> {
    const saved = await forkFieldSet(this.projectId, fieldId);
    this.#run++;
    return this.#replace(saved);
  }
}
