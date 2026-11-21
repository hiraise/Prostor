// Issues, fields and people as the server answers them, for the feature's component tests.
import type {
  ActivityItem,
  ChangeValues,
  Comment,
  HistoryItem,
  Issue,
  IssueField,
  IssuePage,
  ProjectRef,
  UserRef,
} from '#lib/api/schema.gen.ts';

export const ANNA: UserRef = {
  id: 1,
  username: 'anna',
  full_name: 'Anna Admin',
  avatar_color: 0,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

export const BORIS: UserRef = { ...ANNA, id: 2, username: 'boris', full_name: 'Boris Lead' };

export const WEB: ProjectRef = {
  id: 1,
  key: 'WEB',
  name: 'Web',
  logo_color: 2,
  logo_url: null,
  archived: false,
};

/** A field with no value; `values` and friends fill it. */
export function field(
  id: number,
  name: string,
  kind: IssueField['field']['kind'],
  rest: Partial<IssueField> = {},
): IssueField {
  return {
    field: {
      id,
      name,
      kind,
      multi: false,
      builtin: null,
      can_be_empty: true,
      empty_label: 'No value',
    },
    empty_label: 'No value',
    private: false,
    required: false,
    editable: true,
    text: null,
    html: null,
    integer: null,
    float: null,
    date: null,
    date_time: null,
    minutes: null,
    values: [],
    users: [],
    groups: [],
    ...rest,
  };
}

export const PRIORITY = field(11, 'Priority', 'enum', {
  values: [
    {
      id: 101,
      name: 'Major',
      color: 3,
      description: '',
      archived: false,
      resolved: false,
    },
  ],
});
export const ASSIGNEE = field(12, 'Assignee', 'user', {
  field: {
    id: 12,
    name: 'Assignee',
    kind: 'user',
    multi: false,
    builtin: 'assignee',
    can_be_empty: true,
    empty_label: 'Unassigned',
  },
  users: [BORIS],
  empty_label: 'Unassigned',
});
export const ESTIMATE = field(13, 'Estimation', 'duration', { minutes: 90 });
export const DUE = field(14, 'Due date', 'date', { date: '2026-11-02' });
export const NOTES = field(15, 'Steps to reproduce', 'text', {
  text: 'Open the page',
  html: '<p>Open the page</p>\n',
});

export function issue(rest: Partial<Issue> = {}): Issue {
  return {
    id: 501,
    readable_id: 'WEB-12',
    number: 12,
    draft: false,
    project: WEB,
    summary: 'Login button does nothing',
    description: '# Cause\n\nThe handler is missing.',
    description_html: '<h1>Cause</h1>\n<p>The handler is missing.</p>\n',
    refs: { users: [], issues: [], files: [] },
    reporter: ANNA,
    created_at: '2026-10-20T09:00:00Z',
    updated_at: '2026-10-23T15:00:00Z',
    updater: BORIS,
    resolved_at: null,
    audience: { users: [], groups: [] },
    fields: [PRIORITY, ASSIGNEE, ESTIMATE, DUE, NOTES],
    tags: [],
    starred: true,
    voted: false,
    votes: 3,
    watchers: 2,
    can: {
      update: true,
      update_private: false,
      delete: false,
      manage_watchers: false,
      vote: true,
      silent: false,
    },
    subtasks: { total: 0, unresolved: 0 },
    parent: null,
    version: 4,
    text_version: 2,
    ...rest,
  };
}

/** The answer of `GET /issues/{id}/page` for an issue with no comments, changes, links or files. */
export function issuePage(shown: Issue = issue(), rest: Partial<IssuePage> = {}): IssuePage {
  return {
    issue: shown,
    activity: { items: [], older: null, newer: null },
    links: { groups: [], can_link: false },
    attachments: { attachments: [], next: null },
    boards: [],
    ...rest,
  };
}

/** A comment of issue 501 by Anna. */
export function comment(id: number, text: string, rest: Partial<Comment> = {}): Comment {
  return {
    id,
    issue: 501,
    author: ANNA,
    created_at: '2026-10-21T10:00:00Z',
    updated_at: null,
    updater: null,
    text,
    html: `<p>${text}</p>\n`,
    refs: { users: [], issues: [], files: [] },
    audience: { users: [], groups: [] },
    files: [],
    deleted: null,
    can: { update: true, delete: true, restore: false },
    version: 1,
    ...rest,
  };
}

/** Change values with nothing in them; `rest` fills some. */
export function values(rest: Partial<ChangeValues> = {}): ChangeValues {
  return {
    texts: [],
    numbers: [],
    values: [],
    users: [],
    groups: [],
    tags: [],
    files: [],
    links: [],
    boards: [],
    place: null,
    ...rest,
  };
}

/** A change record by Boris. */
export function change(id: number, rest: Partial<HistoryItem> = {}): HistoryItem {
  return {
    id,
    kind: 'field',
    field: null,
    author: BORIS,
    at: '2026-10-21T11:00:00Z',
    origin: 'web',
    silent: false,
    after_comment: null,
    clone_of: null,
    removed: values(),
    added: values(),
    ...rest,
  };
}

/** Stream items: comments and change records. */
export function commentItem(id: number, shown: Comment): ActivityItem {
  return { type: 'comment', id, at: shown.created_at, comment: shown };
}

export function changeItem(record: HistoryItem): ActivityItem {
  return { type: 'change', ...record };
}
