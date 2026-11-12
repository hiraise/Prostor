// A stand-in server for the fields screens' component tests: fields, value sets with values and
// members, and the fields of projects, kept in memory with versions, the way the API answers.
import type {
  AccountSummary,
  Field,
  FieldKind,
  GroupRef,
  ProjectField,
  ProjectRef,
  SetKind,
  UserRef,
  ValueRef,
  ValueSet,
  ValueSetSummary,
} from '#lib/api/schema.gen.ts';

import {
  failure,
  fakeApi,
  type FakeAnswer,
  type FakeRequest,
  type FakeRoute,
} from '../../../test/fake-api.ts';
import type { SetValue } from './fields.ts';

export interface StoredField {
  id: number;
  name: string;
  kind: FieldKind;
  multi: boolean;
  aliases: string[];
  auto_attach: boolean;
  builtin: Field['builtin'];
  can_be_empty: boolean;
  empty_label: string;
  private: boolean;
  setId: number | null;
  defaults: number[];
  version: number;
}

export interface StoredSet {
  id: number;
  name: string;
  kind: SetKind;
  values: SetValue[];
  users: number[];
  groups: number[];
  /** The caller may not change it. */
  locked?: boolean;
  version: number;
}

export interface Attachment {
  fieldId: number;
  setId: number | null;
  can_be_empty: boolean;
  empty_label: string;
  defaults: number[];
  private: boolean;
  version: number;
}

export const PEOPLE: AccountSummary[] = [
  person(7, 'Ann Lee', 'ann'),
  person(8, 'Bob Stone', 'bob'),
  person(9, 'Cleo Park', 'cleo'),
];

export const GROUPS: (GroupRef & { members: number })[] = [
  { id: 30, name: 'Designers', builtin: null, team_of: null, members: 3 },
  { id: 31, name: 'Alpha', builtin: null, team_of: 1, members: 4 },
  { id: 32, name: 'Backend', builtin: null, team_of: null, members: 5 },
];

export const PROJECTS: ProjectRef[] = [
  { id: 1, key: 'ALPHA', name: 'Alpha', archived: false, logo_color: 1, logo_url: null },
  { id: 2, key: 'BETA', name: 'Beta', archived: false, logo_color: 2, logo_url: null },
  { id: 3, key: 'GAMMA', name: 'Gamma', archived: false, logo_color: 3, logo_url: null },
];

function person(id: number, fullName: string, username: string): AccountSummary {
  return {
    id,
    full_name: fullName,
    username,
    avatar_color: id % 8,
    avatar_url: null,
    created_at: '2026-10-01T09:00:00Z',
    disabled: null,
    email: `${username}@example.com`,
    email_verified: true,
    full_profile: true,
    groups: [],
    guest: false,
    invited: false,
    projects: [],
    version: 1,
  };
}

export function userRef(id: number): UserRef {
  const found = PEOPLE.find((item) => item.id === id);
  return {
    id,
    full_name: found?.full_name ?? 'Someone',
    username: found?.username ?? 'someone',
    avatar_color: id % 8,
    avatar_url: null,
    disabled: false,
    kind: 'person',
    pseudonym: null,
  };
}

/** A value with every property at its default. */
export function value(id: number, name: string, extra: Partial<SetValue> = {}): SetValue {
  return {
    id,
    name,
    description: '',
    color: null,
    archived: false,
    resolved: false,
    released: false,
    release_date: null,
    start_date: null,
    assembled_on: null,
    owner: null,
    version: 1,
    ...extra,
  };
}

export function storedField(
  id: number,
  name: string,
  kind: FieldKind,
  extra: Partial<StoredField> = {},
): StoredField {
  return {
    id,
    name,
    kind,
    multi: false,
    aliases: [],
    auto_attach: false,
    builtin: null,
    can_be_empty: true,
    empty_label: '',
    private: false,
    setId: null,
    defaults: [],
    version: 1,
    ...extra,
  };
}

export function storedSet(
  id: number,
  name: string,
  kind: SetKind,
  values: SetValue[] = [],
  extra: Partial<StoredSet> = {},
): StoredSet {
  return { id, name, kind, values, users: [], groups: [], version: 1, ...extra };
}

export function attachment(fieldId: number, extra: Partial<Attachment> = {}): Attachment {
  return {
    fieldId,
    setId: null,
    can_be_empty: true,
    empty_label: '',
    defaults: [],
    private: false,
    version: 1,
    ...extra,
  };
}

/** One board of the boards a field has, as the fake server stores it (BRD-022). */
export interface BoardUse {
  id: number;
  name: string;
  column?: boolean;
  lane?: boolean;
  color?: boolean;
  estimation?: boolean;
  original_estimation?: boolean;
  cards?: boolean;
}

interface Options {
  fields?: StoredField[];
  sets?: StoredSet[];
  /** Fields of projects, in their order. */
  projects?: Record<number, Attachment[]>;
  /** The boards using a field, and how many more the viewer does not view (BRD-022). */
  boards?: Record<number, { items: BoardUse[]; others?: number }>;
  /** Issues holding a value of a field, or one value, by ID (FLD-023, FLD-032, FLD-054). */
  issues?: { fields?: Record<number, number>; values?: Record<number, number> };
  /** Answers that replace the server's for one route (`'POST /value-sets/5/values'`). */
  override?: Record<string, (request: FakeRequest) => FakeAnswer | undefined>;
}

/** Installs the server; returns its state and the recorded calls. */
export function fieldsServer(options: Options = {}) {
  const fields = new Map((options.fields ?? []).map((item) => [item.id, structuredClone(item)]));
  const order = (options.fields ?? []).map((item) => item.id);
  const sets = new Map((options.sets ?? []).map((item) => [item.id, structuredClone(item)]));
  const projects = new Map(
    Object.entries(options.projects ?? {}).map(([id, list]) => [Number(id), structuredClone(list)]),
  );
  let nextId = 500;
  const fieldIssues = (id: number): number => options.issues?.fields?.[id] ?? 0;
  const valueIssues = (id: number): number => options.issues?.values?.[id] ?? 0;

  const projectsUsing = (setId: number): number[] =>
    [...projects].filter(([, list]) => list.some((a) => a.setId === setId)).map(([id]) => id);

  const projectsOfField = (fieldId: number): number[] =>
    [...projects].filter(([, list]) => list.some((a) => a.fieldId === fieldId)).map(([id]) => id);

  function valueRefs(setId: number | null, ids: number[]): ValueRef[] {
    const set = setId === null ? undefined : sets.get(setId);
    return ids.flatMap((id) => {
      const found = set?.values.find((item) => item.id === id);
      return found
        ? [{ id: found.id, name: found.name, color: found.color, archived: found.archived }]
        : [];
    });
  }

  function fieldOf(item: StoredField): Field {
    const set = item.setId === null ? undefined : sets.get(item.setId);
    return {
      id: item.id,
      name: item.name,
      kind: item.kind,
      multi: item.multi,
      aliases: item.aliases,
      auto_attach: item.auto_attach,
      builtin: item.builtin,
      can_be_empty: item.can_be_empty,
      empty_label: item.empty_label,
      private: item.private,
      created_at: '2026-10-01T09:00:00Z',
      default_users: item.kind === 'user' ? item.defaults.map(userRef) : [],
      default_values: item.kind === 'user' ? [] : valueRefs(item.setId, item.defaults),
      project_count: projectsOfField(item.id).length,
      set: set ? { id: set.id, name: set.name, kind: set.kind } : null,
      version: item.version,
    };
  }

  function setOf(item: StoredSet): ValueSet {
    const using = projectsUsing(item.id);
    return {
      id: item.id,
      name: item.name,
      kind: item.kind,
      values: item.kind === 'user' ? [] : item.values,
      users: item.users.map(userRef),
      groups: GROUPS.filter((group) => item.groups.includes(group.id)).map(
        ({ id, name, builtin, team_of }) => ({ id, name, builtin, team_of }),
      ),
      used_in: {
        items: PROJECTS.filter((project) => using.includes(project.id)),
        hidden: using.filter((id) => !PROJECTS.some((project) => project.id === id)).length,
      },
      field_count: [...fields.values()].filter((field) => field.setId === item.id).length,
      editable: !item.locked,
      created_at: '2026-10-01T09:00:00Z',
      version: item.version,
    };
  }

  function summaryOf(item: StoredSet): ValueSetSummary {
    const full = setOf(item);
    return {
      id: item.id,
      name: item.name,
      kind: item.kind,
      projects: full.used_in.items.length + full.used_in.hidden,
      fields: full.field_count,
      version: item.version,
    };
  }

  function projectFieldOf(projectId: number, item: Attachment): ProjectField {
    const field = fields.get(item.fieldId);
    if (!field) throw new Error(`No field ${String(item.fieldId)}`);
    const set = item.setId === null ? undefined : sets.get(item.setId);
    const shared = item.setId === null ? [] : projectsUsing(item.setId);
    return {
      field: {
        id: field.id,
        name: field.name,
        kind: field.kind,
        multi: field.multi,
        builtin: field.builtin,
      },
      aliases: field.aliases,
      can_be_empty: item.can_be_empty,
      empty_label: item.empty_label,
      default_users: field.kind === 'user' ? item.defaults.map(userRef) : [],
      default_values: field.kind === 'user' ? [] : valueRefs(item.setId, item.defaults),
      field_default: field.setId === item.setId,
      private: item.private,
      required: !item.can_be_empty && item.defaults.length === 0,
      set: set ? { id: set.id, name: set.name, kind: set.kind } : null,
      shared_with: shared.filter((id) => id !== projectId).length,
      version: item.version,
    };
  }

  const segments = (request: FakeRequest): string[] => request.path.split('/').slice(1);
  const ok = (body: unknown, status = 200): FakeAnswer => ({ status, body });

  function moveIn<T>(list: T[], key: (item: T) => number, id: number, after: number | null): T[] {
    const moving = list.find((item) => key(item) === id);
    if (moving === undefined) return list;
    const rest = list.filter((item) => key(item) !== id);
    const index = after === null ? 0 : rest.findIndex((item) => key(item) === after) + 1;
    return [...rest.slice(0, index), moving, ...rest.slice(index)];
  }

  function setRoutes(request: FakeRequest): FakeAnswer {
    const [, rawId, part, rawValue, action] = segments(request);
    const method = request.method;
    if (rawId === undefined) {
      if (method === 'GET') {
        const q = request.query.get('q')?.toLowerCase() ?? '';
        const kind = request.query.get('kind');
        const items = [...sets.values()]
          .filter((item) => item.name.toLowerCase().includes(q) && (!kind || item.kind === kind))
          .sort((a, b) => a.name.localeCompare(b.name))
          .map(summaryOf);
        return ok({ items, next_cursor: null });
      }
      const body = request.body as { name: string; kind: SetKind };
      if ([...sets.values()].some((item) => item.kind === body.kind && item.name === body.name)) {
        return failure(400, 'invalid_request', [{ path: 'name', code: 'taken', message: '' }]);
      }
      const created = storedSet(nextId++, body.name, body.kind);
      sets.set(created.id, created);
      return ok(setOf(created), 201);
    }
    const set = sets.get(Number(rawId));
    if (!set) return failure(404, 'not_found');
    if (part === undefined) {
      if (method === 'GET') return ok(setOf(set));
      if (method === 'DELETE') {
        const full = setOf(set);
        if (full.used_in.items.length + full.used_in.hidden + full.field_count > 0) {
          return failure(409, 'conflict');
        }
        sets.delete(set.id);
        return { status: 204 };
      }
      const body = request.body as { name: string; version: number };
      if (body.version !== set.version) return failure(409, 'conflict');
      set.name = body.name;
      set.version += 1;
      return ok(setOf(set));
    }
    if (set.locked) return failure(403, 'forbidden');
    if (part === 'members') {
      const body = request.body as { users?: number[]; groups?: number[] };
      const removing = rawValue === 'remove';
      if (removing) {
        set.users = set.users.filter((id) => !(body.users ?? []).includes(id));
        set.groups = set.groups.filter((id) => !(body.groups ?? []).includes(id));
      } else {
        set.users = [...new Set([...set.users, ...(body.users ?? [])])];
        set.groups = [...new Set([...set.groups, ...(body.groups ?? [])])];
      }
      set.version += 1;
      return ok(setOf(set));
    }
    if (rawValue === undefined) {
      const body = request.body as Partial<SetValue> & { after?: number | null; owner_id?: number };
      if (set.values.some((item) => item.name === body.name)) {
        return failure(400, 'invalid_request', [{ path: 'name', code: 'taken', message: '' }]);
      }
      const { after, owner_id: ownerId, ...props } = body;
      const created = value(nextId++, body.name ?? '', {
        ...props,
        owner: ownerId ? userRef(ownerId) : null,
      });
      set.values = moveIn(
        [...set.values, created],
        (item) => item.id,
        created.id,
        after ?? set.values.at(-1)?.id ?? null,
      );
      set.version += 1;
      return ok(setOf(set));
    }
    const found = set.values.find((item) => item.id === Number(rawValue));
    if (!found) return failure(404, 'not_found');
    if (action === 'usage') return ok({ issues: valueIssues(found.id) });
    if (action === 'move') {
      const body = request.body as { after: number | null };
      set.values = moveIn(set.values, (item) => item.id, found.id, body.after);
    } else if (method === 'DELETE') {
      set.values = set.values.filter((item) => item.id !== found.id);
    } else {
      const body = request.body as Partial<SetValue> & { owner_id?: number | null };
      if (body.version !== found.version) return failure(409, 'conflict');
      if (
        body.name !== undefined &&
        set.values.some((item) => item.id !== found.id && item.name === body.name)
      ) {
        return failure(400, 'invalid_request', [{ path: 'name', code: 'taken', message: '' }]);
      }
      const { owner_id: ownerId, ...props } = body;
      delete props.version;
      Object.assign(found, props);
      if (ownerId !== undefined) found.owner = ownerId === null ? null : userRef(ownerId);
      found.version += 1;
    }
    set.version += 1;
    return ok(setOf(set));
  }

  function fieldRoutes(request: FakeRequest): FakeAnswer {
    const [, rawId, part] = segments(request);
    const method = request.method;
    if (rawId === undefined) {
      if (method === 'GET') {
        const q = request.query.get('q')?.toLowerCase() ?? '';
        const unused = request.query.get('unused') === 'true';
        const items = order
          .map((id) => fields.get(id))
          .filter((item): item is StoredField => item !== undefined)
          .filter(
            (item) =>
              (item.name.toLowerCase().includes(q) ||
                item.aliases.some((alias) => alias.toLowerCase().includes(q))) &&
              (!unused || projectsOfField(item.id).length === 0),
          )
          .map(fieldOf);
        return ok({ items, next_cursor: null });
      }
      const body = request.body as {
        name: string;
        kind: FieldKind;
        multi?: boolean;
        aliases?: string[];
        can_be_empty?: boolean;
        empty_label?: string;
        private?: boolean;
        auto_attach?: boolean;
        default_values?: number[];
        set?: { source: 'new'; name?: string | null } | { source: 'existing'; set_id: number };
      };
      if (/[<>/:]/.test(body.name)) {
        return failure(400, 'invalid_request', [{ path: 'name', code: 'invalid', message: '' }]);
      }
      let setId: number | null = null;
      if (body.set?.source === 'existing') setId = body.set.set_id;
      else if (body.set?.source === 'new') {
        const created = storedSet(nextId++, body.set.name ?? body.name, body.kind as SetKind);
        sets.set(created.id, created);
        setId = created.id;
      }
      const created = storedField(nextId++, body.name, body.kind, {
        multi: body.multi ?? false,
        aliases: body.aliases ?? [],
        can_be_empty: body.can_be_empty ?? true,
        empty_label: body.empty_label ?? '',
        private: body.private ?? false,
        auto_attach: body.auto_attach ?? false,
        defaults: body.default_values ?? [],
        setId,
      });
      fields.set(created.id, created);
      order.push(created.id);
      return ok(fieldOf(created), 201);
    }
    const field = fields.get(Number(rawId));
    if (!field) return failure(404, 'not_found');
    if (part === 'projects') {
      const using = projectsOfField(field.id);
      return ok({
        items: PROJECTS.filter((project) => using.includes(project.id)),
        hidden: using.filter((id) => !PROJECTS.some((project) => project.id === id)).length,
      });
    }
    if (part === 'boards') {
      const boards = options.boards?.[field.id];
      return ok({
        items: (boards?.items ?? []).map((use) => ({
          board: { id: use.id, name: use.name },
          column: use.column ?? false,
          lane: use.lane ?? false,
          color: use.color ?? false,
          estimation: use.estimation ?? false,
          original_estimation: use.original_estimation ?? false,
          cards: use.cards ?? false,
        })),
        others: boards?.others ?? 0,
      });
    }
    if (part === 'deletion-impact') {
      const boards = options.boards?.[field.id];
      return ok({
        projects: projectsOfField(field.id).length,
        issues: fieldIssues(field.id),
        boards: (boards?.items.length ?? 0) + (boards?.others ?? 0),
      });
    }
    if (part === 'move') {
      const body = request.body as { after: number | null };
      const moved = moveIn(order, (id) => id, field.id, body.after);
      order.splice(0, order.length, ...moved);
      return ok(fieldOf(field));
    }
    if (method === 'GET') return ok(fieldOf(field));
    if (method === 'DELETE') {
      fields.delete(field.id);
      order.splice(order.indexOf(field.id), 1);
      for (const list of projects.values()) {
        const index = list.findIndex((item) => item.fieldId === field.id);
        if (index >= 0) list.splice(index, 1);
      }
      return { status: 204 };
    }
    const body = request.body as Partial<StoredField> & {
      version: number;
      default_values?: number[];
    };
    if (body.version !== field.version) return failure(409, 'conflict');
    const { default_values: defaults, ...props } = body;
    Object.assign(field, { ...props, version: field.version });
    if (defaults !== undefined) field.defaults = defaults;
    field.version += 1;
    return ok(fieldOf(field));
  }

  function projectFieldRoutes(request: FakeRequest): FakeAnswer {
    const [, rawProject, , rawField, action] = segments(request);
    const projectId = Number(rawProject);
    const list = projects.get(projectId);
    if (!list) return failure(404, 'not_found');
    const method = request.method;
    const answerAll = (): FakeAnswer =>
      ok({ items: list.map((item) => projectFieldOf(projectId, item)) });
    if (rawField === undefined) {
      if (method === 'GET') return answerAll();
      const body = request.body as
        | { source: 'existing'; field_id: number; set: 'shared' | 'copy' }
        | { source: 'new'; name: string; kind: FieldKind; multi?: boolean; aliases?: string[] };
      let field: StoredField | undefined;
      let setId: number | null = null;
      if (body.source === 'existing') {
        field = fields.get(body.field_id);
        if (!field) return failure(400, 'invalid_request');
        setId = field.setId;
        if (setId !== null && body.set === 'copy') {
          const source = sets.get(setId);
          if (source) {
            const copy = storedSet(nextId++, `${source.name} (copy)`, source.kind, [
              ...source.values.map((item) => ({ ...item, id: nextId++ })),
            ]);
            sets.set(copy.id, copy);
            setId = copy.id;
          }
        }
      } else {
        if (
          [...fields.values()].some((item) => item.name === body.name && item.kind === body.kind)
        ) {
          return failure(400, 'invalid_request', [{ path: 'name', code: 'taken', message: '' }]);
        }
        if (['enum', 'state', 'user', 'version', 'build', 'owned'].includes(body.kind)) {
          const created = storedSet(nextId++, body.name, body.kind as SetKind);
          sets.set(created.id, created);
          setId = created.id;
        }
        field = storedField(nextId++, body.name, body.kind, {
          multi: body.multi ?? false,
          aliases: body.aliases ?? [],
          setId,
        });
        fields.set(field.id, field);
        order.push(field.id);
      }
      const created = attachment(field.id, {
        setId,
        can_be_empty: field.can_be_empty,
        empty_label: field.empty_label,
        private: field.private,
        defaults: body.source === 'existing' && body.set === 'shared' ? field.defaults : [],
      });
      list.push(created);
      return ok(projectFieldOf(projectId, created), 201);
    }
    const item = list.find((entry) => entry.fieldId === Number(rawField));
    if (!item) return failure(404, 'not_found');
    if (action === 'move') {
      const body = request.body as { after: number | null };
      const moved = moveIn(list, (entry) => entry.fieldId, item.fieldId, body.after);
      list.splice(0, list.length, ...moved);
      return answerAll();
    }
    if (action === 'detach-impact') return ok({ issues: fieldIssues(item.fieldId) });
    if (action === 'fork-set') {
      const source = item.setId === null ? undefined : sets.get(item.setId);
      if (!source) return failure(409, 'conflict');
      const copy = storedSet(nextId++, `${source.name} (Alpha)`, source.kind, [
        ...source.values.map((entry) => ({ ...entry })),
      ]);
      sets.set(copy.id, copy);
      item.setId = copy.id;
      item.version += 1;
      return ok(projectFieldOf(projectId, item));
    }
    if (method === 'DELETE') {
      list.splice(list.indexOf(item), 1);
      return { status: 204 };
    }
    const body = request.body as {
      version: number;
      can_be_empty?: boolean;
      empty_label?: string;
      private?: boolean;
      default_values?: number[];
    };
    if (body.version !== item.version) return failure(409, 'conflict');
    if (body.default_values !== undefined) {
      const field = fields.get(item.fieldId);
      const set = item.setId === null ? undefined : sets.get(item.setId);
      const allowed =
        field?.kind === 'user'
          ? (set?.users ?? [])
          : (set?.values.filter((entry) => !entry.archived).map((entry) => entry.id) ?? []);
      if (body.default_values.some((id) => !allowed.includes(id))) {
        return failure(400, 'invalid_request', [
          { path: 'default_values', code: 'invalid', message: '' },
        ]);
      }
      item.defaults = body.default_values;
    }
    if (body.can_be_empty !== undefined) item.can_be_empty = body.can_be_empty;
    if (body.empty_label !== undefined) item.empty_label = body.empty_label;
    if (!item.can_be_empty) item.empty_label = '';
    if (body.private !== undefined) item.private = body.private;
    item.version += 1;
    return ok(projectFieldOf(projectId, item));
  }

  function route(request: FakeRequest): FakeAnswer {
    const key = `${request.method} ${request.path}`;
    const replaced = options.override?.[key]?.(request);
    if (replaced) return replaced;
    const [first, , third] = segments(request);
    if (first === 'value-sets') return setRoutes(request);
    if (first === 'fields') return fieldRoutes(request);
    if (first === 'projects' && third === 'fields') return projectFieldRoutes(request);
    if (first === 'users' && request.method === 'GET') {
      const q = request.query.get('q')?.toLowerCase() ?? '';
      return ok({
        items: PEOPLE.filter((item) => item.full_name.toLowerCase().includes(q)),
        next_cursor: null,
      });
    }
    if (first === 'groups' && request.method === 'GET') {
      const q = request.query.get('q')?.toLowerCase() ?? '';
      return ok({
        items: GROUPS.filter((item) => item.name.toLowerCase().includes(q)).map((item) => ({
          ...item,
          auto_join: false,
          created_at: '2026-10-01T09:00:00Z',
          description: '',
          roles: [],
          version: 1,
        })),
        next_cursor: null,
      });
    }
    return failure(404, 'not_found');
  }

  // Every route goes to `route`: the fake looks routes up by name, and this answers any name.
  const routes = new Proxy({}, { get: () => route }) as Record<string, FakeRoute>;
  const calls = fakeApi(routes);
  return { calls, fields, order, sets, projects, fieldOf, setOf, projectFieldOf };
}

/** The calls that changed something. */
export function writes(calls: readonly FakeRequest[]): FakeRequest[] {
  return calls.filter((call) => call.method !== 'GET');
}
