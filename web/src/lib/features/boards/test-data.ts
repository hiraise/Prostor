// Fixtures of board tests: a kanban-like board over one project with three state columns.
import type {
  AccessMap,
  Board,
  BoardColumn,
  BoardView,
  CardIssue,
  FieldRef,
  HeldValue,
  ProjectRef,
  Session,
  Sprint,
  UserRef,
} from '#lib/api/schema.gen.ts';

export const SESSION: Session = {
  user: {
    id: 1,
    username: 'anna',
    full_name: 'Anna Admin',
    email: 'anna@example.com',
    email_verified: true,
    avatar_color: 0,
    avatar_url: null,
    version: 1,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

export const ANNA: UserRef = {
  id: 1,
  full_name: 'Anna Admin',
  username: 'anna',
  avatar_color: 0,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

export const BORIS: UserRef = { ...ANNA, id: 2, full_name: 'Boris Lead', username: 'boris' };

/** Reads and updates issues in project 1. */
export const MEMBER: AccessMap = {
  global: [],
  projects: [
    {
      project_id: 1,
      permissions: ['project.basic', 'issue.read', 'issue.update', 'project.update'],
    },
  ],
  capabilities: [],
};

export const WEB: ProjectRef = {
  id: 1,
  name: 'Web',
  key: 'WEB',
  archived: false,
  logo_color: 0,
  logo_url: null,
};

export const STATE: FieldRef = {
  id: 5,
  name: 'State',
  kind: 'state',
  multi: false,
  builtin: 'state',
};
export const ASSIGNEE: FieldRef = {
  id: 6,
  name: 'Assignee',
  kind: 'user',
  multi: false,
  builtin: 'assignee',
};
export const PRIORITY: FieldRef = {
  id: 7,
  name: 'Priority',
  kind: 'enum',
  multi: false,
  builtin: 'priority',
};
export const ESTIMATION: FieldRef = {
  id: 8,
  name: 'Estimation',
  kind: 'duration',
  multi: false,
  builtin: 'estimation',
};

export function value(id: number, name: string, extra: Partial<HeldValue> = {}): HeldValue {
  return { id, name, archived: false, color: null, description: '', resolved: false, ...extra };
}

export const OPEN = value(51, 'Open');
export const DOING = value(52, 'In Progress');
export const DONE = value(53, 'Done', { resolved: true });
export const MAJOR = value(71, 'Major', { color: 3 });

export function column(id: number, held: HeldValue, extra: Partial<BoardColumn> = {}): BoardColumn {
  return { id, value: held, wip_min: null, wip_max: null, ...extra };
}

export const COLUMNS = [column(1, OPEN), column(2, DOING, { wip_max: 1 }), column(3, DONE)];

export const NO_LANES: Board['lanes'] = {
  mode: 'none',
  field: null,
  attribute: null,
  values: [],
  other: 'bottom',
  new_card_value: null,
  labels: [],
};

export const NO_CARDS: Board['cards'] = {
  fields: [],
  initials: [],
  color: 'none',
  color_field: null,
  estimation: null,
  original_estimation: null,
};

/** Cards show the priority and the estimation (BRD-059, BRD-062). */
export const CARDS: Board['cards'] = {
  ...NO_CARDS,
  fields: [PRIORITY],
  estimation: ESTIMATION,
};

export function board(extra: Partial<Board> = {}): Board {
  return {
    id: 9,
    name: 'Team',
    column_field: STATE,
    columns: COLUMNS,
    lanes: NO_LANES,
    cards: CARDS,
    health: [],
    created_at: '2026-10-25T09:00:00Z',
    updated_at: '2026-10-25T09:00:00Z',
    favorite: false,
    mode: 'manual',
    auto_sprint: null,
    backlog: null,
    query: null,
    query_problems: [],
    owner: ANNA,
    preset: 'kanban',
    projects: [{ project: WEB, color: null }],
    rights: { delete: true, edit: true, share: true, transfer: true },
    sharing: {
      view: { users: [], groups: [] },
      edit: { users: [], groups: [] },
      view_readers: true,
      edit_updaters: false,
    },
    sprints_on: false,
    version: 3,
    ...extra,
  };
}

export function issue(id: number, summary: string, extra: Partial<CardIssue> = {}): CardIssue {
  return {
    id,
    readable_id: `WEB-${String(id)}`,
    project: 1,
    summary,
    excerpt: '',
    resolved_at: null,
    tags: [],
    fields: [],
    files: 0,
    restricted: false,
    starred: false,
    can_update: true,
    version: 1,
    ...extra,
  };
}

/** The sprint a board without sprints keeps its cards in. */
export const INTERNAL: Sprint = {
  id: 7,
  name: '',
  internal: true,
  current: true,
  archived: false,
  finished: false,
  goal: '',
  starts_on: null,
  ends_on: null,
  finished_at: null,
  version: 1,
};

/**
 * Two cards in Open (one assigned, with a priority, a tag, an estimation); two in progress, over
 * the limit of 1, one of them not shown yet; one done.
 */
export function view(extra: Partial<BoardView> = {}): BoardView {
  const issues = [
    issue(101, 'Login form', {
      excerpt: '## Why\nPeople **cannot** sign in.\n\nSee [the log](http://x).',
      tags: [{ id: 1, name: 'ui', color: 2 }],
      fields: [
        { field: STATE.id, ids: [OPEN.id], text: null, number: null },
        { field: ASSIGNEE.id, ids: [BORIS.id], text: null, number: null },
        { field: PRIORITY.id, ids: [MAJOR.id], text: null, number: null },
        { field: ESTIMATION.id, ids: [], text: null, number: 90 },
      ],
    }),
    issue(102, 'Logout'),
    issue(103, 'Search'),
    issue(104, 'Done already', { resolved_at: '2026-10-24T09:00:00Z' }),
  ];
  return {
    board: board(),
    sprint: INTERNAL,
    sprints: [],
    columns: [
      { column: 1, count: 2, estimate: null, wip: 'within' },
      { column: 2, count: 2, estimate: null, wip: 'over' },
      { column: 3, count: 1, estimate: null, wip: 'within' },
    ],
    cards: [
      { issue: 101, column: 1, lane: null, rank: 'a' },
      { issue: 102, column: 1, lane: null, rank: 'b' },
      { issue: 103, column: 2, lane: null, rank: 'a' },
      { issue: 104, column: 3, lane: null, rank: 'a' },
    ],
    cells: [{ column: 2, lane: null, total: 2, next_cursor: 'c2' }],
    lanes: [],
    lanes_next: null,
    lanes_total: 0,
    issues,
    refs: {
      fields: [STATE, ASSIGNEE, PRIORITY, ESTIMATION],
      values: [OPEN, DOING, DONE, MAJOR],
      users: [BORIS],
      groups: [],
    },
    totals: { count: 5, estimate: null },
    sort: null,
    notes: [],
    live_topic: 'board:9',
    profile: { favorite: false, card_size: 1, collapsed_columns: [], collapsed_lanes: [] },
    ...extra,
  };
}

/** A sprint of a sprint board. */
export function sprint(id: number, name: string, extra: Partial<Sprint> = {}): Sprint {
  return { ...INTERNAL, id, name, internal: false, current: false, ...extra };
}
