// Saving one field of an issue from its editor (ISS-019): the row shows the new value at once and
// rolls back when the server refuses. Field changes are last-write-wins per field (ISS-051), so
// no version is sent.
import { ApiError, api } from '#lib/api/client.ts';
import { showRuleMessages } from '#lib/ui/rule-messages.ts';
import type { FieldValueInput, Issue, IssueField } from '#lib/api/schema.gen.ts';
import { store } from '#lib/store/index.ts';

/** What the endpoint of a change takes. */
export type FieldRequest =
  /** `PUT …/fields/{field}`: the whole value; `null` clears it. */
  | { kind: 'set'; value: FieldValueInput | null }
  /** `POST …/fields/{field}/add` or `/remove`: values of a multi-valued field. */
  | { kind: 'add' | 'remove'; ids: number[] };

/** A refusal carried past the store, which would otherwise read `conflict` as a stale version. */
class Refused extends Error {
  constructor(readonly original: ApiError) {
    super(original.message);
  }
}

/** Sends a field change without touching the issue store (board cards keep their own, D-044). */
export async function sendField(
  issueId: number,
  fieldId: number,
  request: FieldRequest,
): Promise<Issue> {
  const path = { id: issueId, field_id: fieldId };
  const answer =
    request.kind === 'set'
      ? await api.put('/issues/{id}/fields/{field_id}', { path, body: { value: request.value } })
      : await api.post(
          request.kind === 'add'
            ? '/issues/{id}/fields/{field_id}/add'
            : '/issues/{id}/fields/{field_id}/remove',
          { path, body: { ids: request.ids } },
        );
  showRuleMessages(answer.messages);
  return answer.issue;
}

/** Shows `next` in the issue's row and saves it; throws the server's refusal after rolling back. */
export async function saveField(
  issueId: number,
  next: IssueField,
  request: FieldRequest,
): Promise<void> {
  const row = store.entities.table('issue').get(issueId);
  if (!row) return;
  const fields = row.fields.map((entry) => (entry.field.id === next.field.id ? next : entry));
  try {
    await store.update('issue', issueId, { fields }, async () => {
      try {
        return await sendField(issueId, next.field.id, request);
      } catch (error) {
        throw error instanceof ApiError && error.code === 'conflict' ? new Refused(error) : error;
      }
    });
  } catch (error) {
    throw error instanceof Refused ? error.original : error;
  }
}
