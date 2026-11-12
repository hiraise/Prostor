-- Boards (step 1.8a, D-044): the board, its projects, columns, sprints with their cards, and each
-- person's view of a board.
create table boards (
    id bigint generated always as identity primary key,
    name text not null,
    owner_id bigint not null references users (id),
    preset text not null check (preset in ('scrum', 'kanban', 'custom')),
    sprints_on boolean not null,
    -- How issues join (BRD-025…BRD-028): `manual`, `auto` (new issues join), `query` (1.8b).
    mode text not null default 'manual' check (mode in ('manual', 'auto', 'query')),
    -- The stored form (D-038), mode `query` only.
    query jsonb,
    -- Target of auto-add on sprint boards (1.8b).
    auto_sprint_id bigint,
    column_field_id bigint references fields (id) on delete set null,
    -- Card settings (1.8c fills them; 1.8a creates the columns).
    card_fields bigint[] not null default '{}',   -- shown on cards, in order
    card_initials bigint[] not null default '{}', -- of those, shown as a first letter only
    color_mode text not null default 'field' check (color_mode in ('none', 'project', 'field')),
    color_field_id bigint references fields (id) on delete set null,
    estimation_field_id bigint references fields (id) on delete set null,
    original_estimation_field_id bigint references fields (id) on delete set null,
    backlog_search_id bigint references saved_searches (id) on delete set null,
    view_users bigint[] not null default '{}',
    view_groups bigint[] not null default '{}',
    edit_users bigint[] not null default '{}',
    edit_groups bigint[] not null default '{}',
    view_readers boolean not null default false,  -- everyone who reads issues in every board project
    edit_updaters boolean not null default false, -- everyone who updates issues in a board project
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    version bigint not null default 1
);
create index boards_owner_idx on boards (owner_id);
create index boards_view_users_idx on boards using gin (view_users)
    where cardinality(view_users) > 0;
create index boards_view_groups_idx on boards using gin (view_groups)
    where cardinality(view_groups) > 0;
create index boards_edit_users_idx on boards using gin (edit_users)
    where cardinality(edit_users) > 0;
create index boards_edit_groups_idx on boards using gin (edit_groups)
    where cardinality(edit_groups) > 0;
create index boards_backlog_idx on boards (backlog_search_id) where backlog_search_id is not null;

create table board_projects (
    board_id bigint not null references boards (id) on delete cascade,
    project_id bigint not null references projects (id) on delete cascade,
    color smallint,                               -- card color by project (BRD-060), 1.8c
    primary key (board_id, project_id)
);
create index board_projects_project_idx on board_projects (project_id);

create table board_columns (
    id bigint generated always as identity primary key,
    board_id bigint not null references boards (id) on delete cascade,
    value_id bigint not null references field_values (id) on delete cascade,
    rank text collate "C" not null,
    wip_min integer,
    wip_max integer,
    unique (board_id, value_id)
);
create index board_columns_value_idx on board_columns (value_id);

create table sprints (
    id bigint generated always as identity primary key,
    board_id bigint not null references boards (id) on delete cascade,
    name text not null,
    goal text not null default '',
    starts_on date,                               -- calendar dates in the instance time zone
    ends_on date,
    finished_at timestamptz,                      -- finished by hand (D-013)
    archived_at timestamptz,
    internal boolean not null default false,      -- the hidden sprint of a sprintless board
    order_version bigint not null default 0,      -- grows with each reorder (live)
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    version bigint not null default 1
);
create index sprints_board_idx on sprints (board_id);
create unique index sprints_name_key on sprints (board_id, lower(name)) where not internal;
create unique index sprints_internal_key on sprints (board_id) where internal;
alter table boards add foreign key (auto_sprint_id) references sprints (id) on delete set null;

-- Cards: the issues of a sprint and their shared order (BRD-069). In query mode a row only holds
-- a rank: membership comes from the query.
create table sprint_issues (
    sprint_id bigint not null references sprints (id) on delete cascade,
    issue_id bigint not null references issues (id) on delete cascade,
    rank text collate "C",
    added_at timestamptz not null default now(),
    primary key (sprint_id, issue_id)
);
create index sprint_issues_issue_idx on sprint_issues (issue_id);
create index sprint_issues_rank_idx on sprint_issues (sprint_id, rank);

-- Each person's view of a board (BRD-001, BRD-002, BRD-047, BRD-058): synced across devices.
create table board_profiles (
    user_id bigint not null references users (id) on delete cascade,
    board_id bigint not null references boards (id) on delete cascade,
    favorite boolean not null default false,
    card_size smallint not null default 1 check (card_size between 0 and 3),
    collapsed_columns bigint[] not null default '{}',
    collapsed_lanes text[] not null default '{}',  -- lane keys
    visited_at timestamptz,
    primary key (user_id, board_id)
);
create index board_profiles_visited_idx on board_profiles (user_id, visited_at desc);
create index board_profiles_board_idx on board_profiles (board_id);

-- Boards a draft skips when it is published (ISS-007, 1.8b).
alter table issues add column board_skips bigint[] not null default '{}';

-- Board sockets get the events of the boards they show (BRD-114); a card reorder carries its
-- place (BRD-115), the one patch of the log.
alter table live_events add column boards bigint[] not null default '{}';
alter table live_events add column patch jsonb;

-- Board records in the history (HIST-002): `{"board": {"id", "name"}, "sprint": {"id", "name"}?}`
-- removed and/or added, with the names they had.
alter table issue_history drop constraint issue_history_kind_check;
alter table issue_history add constraint issue_history_kind_check check (kind in (
    'created', 'summary', 'description', 'field', 'project', 'tags', 'visibility',
    'resolved', 'reopened', 'deleted',
    'comment', 'comment_edited', 'comment_deleted', 'comment_restored',
    'attachments', 'links', 'boards'));
