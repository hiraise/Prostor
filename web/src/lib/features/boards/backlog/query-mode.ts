// The field changes that put an issue on a query board (BRD-028, owner decision 2), as lines
// «поле: было → станет». «Было» comes from the issue as the person sees it now (a backlog row),
// «станет» from the preview's answer: the server describes the issue with the changes made.
import type { CardField, CardRefs, Issue, IssueField, PlacedCard } from '#lib/api/schema.gen.ts';

import * as m from '../messages.ts';

export interface ChangeLine {
  field: string;
  was: string;
  now: string;
}

function or(text: string): string {
  return text === '' ? m.emptyValue() : text;
}

/** A card field by name, from the refs of the answer. */
function cardText(refs: CardRefs, value: CardField | undefined): string {
  if (!value) return m.emptyValue();
  if (value.ids.length > 0) {
    const kind = refs.fields.find((one) => one.id === value.field)?.kind;
    return value.ids
      .map((id) => {
        if (kind === 'user') return refs.users.find((one) => one.id === id)?.full_name ?? '';
        if (kind === 'group') return refs.groups.find((one) => one.id === id)?.name ?? '';
        return refs.values.find((one) => one.id === id)?.name ?? '';
      })
      .join(', ');
  }
  if (value.text !== null) return or(value.text);
  if (value.number !== null) return String(value.number);
  return m.emptyValue();
}

/** An issue field as text. */
function issueText(value: IssueField | undefined): string {
  if (!value) return m.emptyValue();
  const names = [
    ...value.values.map((one) => one.name),
    ...value.users.map((one) => one.full_name),
    ...value.groups.map((one) => one.name),
  ];
  if (names.length > 0) return names.join(', ');
  const plain =
    value.text ??
    value.date ??
    value.date_time ??
    [value.integer, value.float, value.minutes].find((one) => one !== null)?.toString() ??
    '';
  return or(plain);
}

/** The lines of a preview, in the order the server gave the changes. */
export function changeLines(
  placed: Pick<PlacedCard, 'changes' | 'issue' | 'refs'>,
  before: Pick<Issue, 'fields' | 'tags'> | null,
): ChangeLine[] {
  const after = placed.issue;
  const refs = placed.refs;
  return placed.changes.map((change) => {
    if (change.kind === 'tag') {
      const was = or((before?.tags ?? []).map((one) => one.name).join(', '));
      const now = or((after?.tags ?? []).map((one) => one.name).join(', '));
      return { field: m.tagsField(), was, now };
    }
    const field = refs.fields.find((one) => one.id === change.field)?.name ?? '';
    const was = issueText(before?.fields.find((one) => one.field.id === change.field));
    const now =
      change.kind === 'clear'
        ? m.emptyValue()
        : cardText(
            refs,
            after?.fields.find((one) => one.field === change.field),
          );
    return { field, was, now };
  });
}
