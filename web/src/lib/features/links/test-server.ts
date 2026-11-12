// A stand-in server for the link types screens' component tests: the types kept in memory with
// versions and usage counts, checked the way the API checks them.
import type { LinkDirection, LinkType } from '#lib/api/schema.gen.ts';

import {
  failure,
  fakeApi,
  type FakeAnswer,
  type FakeRequest,
  type FakeRoute,
} from '../../../test/fake-api.ts';

/** A type with defaults for what a test does not care about. */
export function linkType(
  id: number,
  name: string,
  source: string,
  target: string | null,
  extra: Partial<LinkType> = {},
): LinkType {
  return {
    id,
    name,
    source_name: source,
    target_name: target,
    direction: target === null ? 'undirected' : 'directed',
    builtin: null,
    locked: false,
    deleted_at: null,
    usage: 0,
    version: 1,
    ...extra,
  };
}

/** The four built-in types (step-1.4 plan §2) and one of the instance's own. */
export function builtinTypes(): LinkType[] {
  return [
    linkType(1, 'Related', 'related to', null, { builtin: 'related', usage: 4 }),
    linkType(2, 'Dependency', 'blocks', 'blocked by', { builtin: 'dependency', usage: 12 }),
    linkType(3, 'Duplicate', 'has duplicate', 'duplicate of', {
      builtin: 'duplicate',
      direction: 'hierarchy',
      locked: true,
      usage: 1,
    }),
    linkType(4, 'Subtask', 'parent of', 'child of', {
      builtin: 'subtask',
      direction: 'hierarchy',
      locked: true,
      usage: 30,
    }),
  ];
}

interface Options {
  types?: LinkType[];
  /** Answers like a viewer without `system.read`: no deleted types, no usage. */
  limited?: boolean;
  /** Answers that replace the server's for one route (`'PATCH /link-types/5'`). */
  override?: Record<string, (request: FakeRequest) => FakeAnswer | undefined>;
}

interface Body {
  name?: string | null;
  source_name?: string | null;
  target_name?: string | null;
  direction?: LinkDirection | null;
  version?: number;
}

/** Installs the server; returns its state and the recorded calls. */
export function linkTypesServer(options: Options = {}) {
  const types = new Map((options.types ?? builtinTypes()).map((t) => [t.id, structuredClone(t)]));
  let nextId = 100;
  const ok = (body: unknown, status = 200): FakeAnswer => ({ status, body });

  function shown(type: LinkType): LinkType {
    return options.limited ? { ...type, usage: null } : type;
  }

  /** Field problems of names, the way the server finds them. */
  function problems(candidate: LinkType, except: number | null): unknown[] {
    const found: { path: string; code: string; message: string }[] = [];
    const others = [...types.values()].filter((type) => type.id !== except);
    const sides = new Set(
      others.flatMap((type) => [type.source_name, type.target_name ?? ''].filter(Boolean)),
    );
    const lower = new Set([...sides].map((side) => side.toLowerCase()));
    const check = (path: string, value: string | null): void => {
      if (value === null) return;
      if (!value.trim()) found.push({ path, code: 'required', message: '' });
      else if (value.length > 50) found.push({ path, code: 'too_long', message: '' });
      else if (/[<>/:\n]/.test(value)) found.push({ path, code: 'invalid', message: '' });
      else if (path === 'name' && others.some((type) => type.name === value)) {
        found.push({ path, code: 'taken', message: '' });
      } else if (path !== 'name' && lower.has(value.toLowerCase())) {
        found.push({ path, code: 'taken', message: '' });
      }
    };
    check('name', candidate.name);
    check('source_name', candidate.source_name);
    if (candidate.direction !== 'undirected') {
      if (candidate.target_name === null) {
        found.push({ path: 'target_name', code: 'required', message: '' });
      } else check('target_name', candidate.target_name);
    }
    return found;
  }

  function route(request: FakeRequest): FakeAnswer {
    const override = options.override?.[`${request.method} ${request.path}`]?.(request);
    if (override) return override;
    const [, , rawId, action] = request.path.split('/');
    const body = (request.body ?? {}) as Body;
    if (rawId === undefined) {
      if (request.method === 'GET') {
        const items = [...types.values()]
          .filter((type) => !options.limited || type.deleted_at === null)
          .sort((a, b) => a.id - b.id)
          .map(shown);
        return ok({ items });
      }
      const direction = body.direction ?? 'directed';
      const created = linkType(
        nextId,
        body.name ?? '',
        body.source_name ?? '',
        direction === 'undirected' ? null : (body.target_name ?? null),
        { direction },
      );
      const found = problems(created, null);
      if (found.length > 0) return failure(400, 'invalid_request', found);
      nextId += 1;
      types.set(created.id, created);
      return ok(created, 201);
    }
    const type = types.get(Number(rawId));
    if (!type) return failure(404, 'not_found');
    if (action === 'impact') return ok({ links: type.usage ?? 0 });
    if (action === 'restore') {
      if (type.deleted_at === null) return failure(409, 'conflict');
      const found = problems(type, type.id);
      if (found.length > 0) return failure(400, 'invalid_request', found);
      type.deleted_at = null;
      type.version += 1;
      return ok(type);
    }
    if (type.locked) return failure(409, 'link_type_locked');
    if (request.method === 'DELETE') {
      type.deleted_at = '2026-10-23T09:00:00Z';
      type.version += 1;
      return { status: 204 };
    }
    // PATCH
    if (body.version !== type.version || type.deleted_at !== null) {
      return failure(409, 'conflict');
    }
    const direction = body.direction ?? type.direction;
    const next: LinkType = {
      ...type,
      name: body.name ?? type.name,
      source_name: body.source_name ?? type.source_name,
      target_name: direction === 'undirected' ? null : (body.target_name ?? type.target_name),
      direction,
    };
    const found = problems(next, type.id);
    if (found.length > 0) return failure(400, 'invalid_request', found);
    Object.assign(type, next, { version: type.version + 1 });
    return ok(type);
  }

  // Every route goes to `route`: the fake looks routes up by name, and this answers any name.
  const routes = new Proxy({}, { get: () => route }) as Record<string, FakeRoute>;
  const calls = fakeApi(routes);
  return { calls, types };
}

/** The calls that changed something. */
export function writes(calls: readonly FakeRequest[]): FakeRequest[] {
  return calls.filter((call) => call.method !== 'GET');
}
