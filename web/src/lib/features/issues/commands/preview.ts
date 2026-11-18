// The preview's rows (plan 2.1 §6, CMD-028): every planned operation as the dialog shows it —
// «Исправления: +2025.3 −2024.3», the move with its mapping note, the comment — and what the
// dialog does about them: `delete` switches to the confirm step (§6), the comment says when its
// field is empty.
import type { CommandAssist, PlannedOperation } from '#lib/api/schema.gen.ts';

/** One value of a row, with the sign the preview prefixes: `+` joins, `−` leaves, `null` names. */
export interface RowPart {
  text: string;
  sign: '+' | '-' | null;
}

/** One preview row. */
export interface PreviewRow {
  /** The kind, for tests and the confirm step's check. */
  kind: PlannedOperation['kind'];
  /** What the row is about: the field, «Тег», «Перенос в». */
  title: string;
  /** The named values with their signs; empty for the nameless kinds. */
  parts: RowPart[];
  /** A note under the values (the move's mapping). */
  note?: string;
}

/** Whether the plan deletes: the dialog switches to its confirm step (§6). */
export function deletes(operations: readonly PlannedOperation[]): boolean {
  return operations.some((operation) => operation.kind === 'delete');
}

/** Builds the rows: the operations of the assist's answer in command order, each with the marks
 * the answer carries (the direction, the removals of a plain set on a multi-value field). */
export function rows(
  answer: CommandAssist,
  titles: (operation: PlannedOperation) => string,
  moveNote: string,
): PreviewRow[] {
  return answer.operations.map((operation) => {
    const row: PreviewRow = {
      kind: operation.kind,
      title: titles(operation),
      parts: [],
    };
    switch (operation.kind) {
      case 'set_field': {
        // names[0] is the field (the title); the rest are the values it names.
        for (const name of (operation.names ?? []).slice(1)) {
          row.parts.push({ text: name, sign: signOf(operation.mode) });
        }
        for (const name of operation.removals ?? []) {
          row.parts.push({ text: name, sign: '-' });
        }
        break;
      }
      case 'tag':
      case 'visibility':
      case 'star':
      case 'vote': {
        for (const name of operation.names ?? []) {
          row.parts.push({ text: name, sign: signOf(operation.mode) });
        }
        break;
      }
      case 'move':
      case 'link':
      case 'duplicate':
      case 'board': {
        for (const name of operation.names ?? []) {
          row.parts.push({
            text: name,
            sign: operation.kind === 'link' ? signOf(operation.mode) : null,
          });
        }
        if (operation.kind === 'move') row.note = moveNote;
        break;
      }
      default:
        break;
    }
    return row;
  });
}

/** The `+`/`−` the mode prefixes; a plain `set` marks its values `+` too — what joins the issue. */
function signOf(mode: string | null | undefined): '+' | '-' | null {
  if (mode === 'add' || mode === 'set') return '+';
  if (mode === 'remove') return '-';
  return null;
}
