// A card's field in the two forms (D-044): the compact one cards travel in, and the one the issue
// editors take (BRD-063). Cards hold only fields with a value; an empty one becomes an empty
// entry so it can be set from the card.
import type {
  CardField,
  CardRefs,
  FieldRef,
  GroupRef,
  HeldValue,
  IssueField,
  UserRef,
} from '#lib/api/schema.gen.ts';
import { getLocale } from '#lib/i18n/locale.svelte.ts';
import { formatDuration } from '#lib/time/duration.ts';

/** What names the IDs of a card field. */
export interface Names {
  values: ReadonlyMap<number, HeldValue>;
  users: ReadonlyMap<number, UserRef>;
  groups: ReadonlyMap<number, GroupRef>;
}

function known<T>(ids: number[], map: ReadonlyMap<number, T>): T[] {
  return ids.map((id) => map.get(id)).filter((one): one is T => one !== undefined);
}

/** The editor's form of a card field. */
export function entryOf(
  ref: FieldRef,
  field: CardField | undefined,
  editable: boolean,
  names: Names,
): IssueField {
  const ids = field?.ids ?? [];
  const text = field?.text ?? null;
  const number = field?.number ?? null;
  const kind = ref.kind;
  return {
    field: ref,
    editable,
    // The field's own setting: a project attachment may override it, but the card payload
    // carries no attachment — the server still refuses a clear it must refuse.
    empty_label: ref.empty_label,
    private: false,
    required: !ref.can_be_empty,
    html: null,
    values: kind === 'user' || kind === 'group' ? [] : known(ids, names.values),
    users: kind === 'user' ? known(ids, names.users) : [],
    groups: kind === 'group' ? known(ids, names.groups) : [],
    text: kind === 'string' ? text : null,
    date: kind === 'date' ? text : null,
    date_time: kind === 'date_time' ? text : null,
    integer: kind === 'integer' ? number : null,
    float: kind === 'float' ? number : null,
    minutes: kind === 'duration' ? number : null,
  };
}

/** The card's form of an edited field, with what it names. */
export function cardFieldOf(entry: IssueField): { field: CardField; refs: Partial<CardRefs> } {
  return {
    field: {
      field: entry.field.id,
      ids: [
        ...entry.values.map((one) => one.id),
        ...entry.users.map((one) => one.id),
        ...entry.groups.map((one) => one.id),
      ],
      text: entry.text ?? entry.date ?? entry.date_time,
      number: entry.integer ?? entry.float ?? entry.minutes,
    },
    refs: { values: entry.values, users: entry.users, groups: entry.groups },
  };
}

/** A number of a field as cards show it: durations as `1w 2d`, others as they are. */
export function formatNumber(value: number, kind: FieldRef['kind'] | undefined): string {
  if (kind === 'duration') return formatDuration(value, getLocale());
  return new Intl.NumberFormat(getLocale(), { maximumFractionDigits: 2 }).format(value);
}
