-- Issues (step 1.3): issues and drafts, former IDs, field values in tables by value type, change
-- history, tags, watchers and votes. Design: decisions.md D-029; field values D-022, their query
-- rules D-024; visibility architecture.md §4.

-- Issues and drafts (ISS-001, ISS-002). A draft has no number and only its reporter sees it.
create table issues (
    id bigint generated always as identity primary key,
    -- Content of a project being deleted goes first (PRJ-064), so no cascade here.
    project_id bigint not null references projects (id),
    -- Set when the issue is published, unique in its project, never reused (PRJ-004).
    number bigint,
    summary text not null default '',
    -- Markdown.
    description text not null default '',
    reporter_id bigint not null references users (id),
    -- Publication time for issues, start time for drafts.
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    updated_by bigint references users (id),
    -- Set while every state field of the project holds a resolved value (ISS-023).
    resolved_at timestamptz,
    -- Deleted issues stay whole for the trash (ISS-028, ISS-029) and are out of every list.
    deleted_at timestamptz,
    deleted_by bigint references users (id),
    -- The audience the issue is limited to (ISS-041): accounts and groups (teams included). Both
    -- empty: no restriction.
    audience_users bigint[] not null default '{}',
    audience_groups bigint[] not null default '{}',
    -- Number of votes (ISS-032), kept with each vote so lists can sort by it.
    votes integer not null default 0 check (votes >= 0),
    -- Grows with every change of the issue.
    version bigint not null default 1,
    -- Grows with every change of the summary or description: edits of the texts carry the one
    -- they started from (ISS-051).
    text_version bigint not null default 1,
    check (number is null or number >= 1),
    check (deleted_at is null or number is not null)
);
create unique index issues_number_key on issues (project_id, number);
-- Published issues of a project, newest first; what lists and boards start from.
create index issues_project_idx on issues (project_id, id)
    where number is not null and deleted_at is null;
-- Unresolved issues of a project (D-024: "unresolved" compiles to this condition).
create index issues_unresolved_idx on issues (project_id, id)
    where resolved_at is null and number is not null and deleted_at is null;
create index issues_reporter_idx on issues (reporter_id);
create index issues_drafts_idx on issues (reporter_id, id) where number is null;
create index issues_updated_by_idx on issues (updated_by) where updated_by is not null;
create index issues_deleted_by_idx on issues (deleted_by) where deleted_by is not null;
-- Replacing deleted accounts and groups in audiences (architecture.md §6).
create index issues_audience_users_idx on issues using gin (audience_users)
    where cardinality(audience_users) > 0;
create index issues_audience_groups_idx on issues using gin (audience_groups)
    where cardinality(audience_groups) > 0;

-- Former IDs of issues that moved to another project (ISS-003). A project's former keys are in
-- project_keys, so `OLD-12` resolves through both.
create table issue_aliases (
    project_id bigint not null references projects (id) on delete cascade,
    number bigint not null,
    issue_id bigint not null references issues (id) on delete cascade,
    primary key (project_id, number)
);
create index issue_aliases_issue_idx on issue_aliases (issue_id);

-- Field values on issues, one table per value type (D-022). `field_id` has no foreign key:
-- values of a deleted field are erased by a background job (FLD-023), and field IDs are never
-- reused. Every table has extended statistics on (field, value): a value belongs to one field,
-- and without them the planner multiplies the two selectivities (D-024).

-- Values of enumerated fields: `value` is a field_values ID for enum, state, version, build and
-- owned fields, an account ID for user fields, a group ID for group fields. Several rows for a
-- multi-valued field.
create table issue_ref_values (
    issue_id bigint not null references issues (id) on delete cascade,
    field_id bigint not null,
    value bigint not null,
    primary key (issue_id, field_id, value)
);
create index issue_ref_values_value_idx on issue_ref_values (field_id, value, issue_id);
create statistics issue_ref_values_field_value (mcv, dependencies, ndistinct)
    on field_id, value from issue_ref_values;

-- Integers and durations (minutes).
create table issue_int_values (
    issue_id bigint not null references issues (id) on delete cascade,
    field_id bigint not null,
    value bigint not null,
    primary key (issue_id, field_id)
);
create index issue_int_values_value_idx on issue_int_values (field_id, value, issue_id);
create statistics issue_int_values_field_value (mcv, dependencies, ndistinct)
    on field_id, value from issue_int_values;

create table issue_float_values (
    issue_id bigint not null references issues (id) on delete cascade,
    field_id bigint not null,
    value double precision not null,
    primary key (issue_id, field_id)
);
create index issue_float_values_value_idx on issue_float_values (field_id, value, issue_id);
create statistics issue_float_values_field_value (mcv, dependencies, ndistinct)
    on field_id, value from issue_float_values;

create table issue_date_values (
    issue_id bigint not null references issues (id) on delete cascade,
    field_id bigint not null,
    value date not null,
    primary key (issue_id, field_id)
);
create index issue_date_values_value_idx on issue_date_values (field_id, value, issue_id);
create statistics issue_date_values_field_value (mcv, dependencies, ndistinct)
    on field_id, value from issue_date_values;

create table issue_datetime_values (
    issue_id bigint not null references issues (id) on delete cascade,
    field_id bigint not null,
    value timestamptz not null,
    primary key (issue_id, field_id)
);
create index issue_datetime_values_value_idx on issue_datetime_values (field_id, value, issue_id);
create statistics issue_datetime_values_field_value (mcv, dependencies, ndistinct)
    on field_id, value from issue_datetime_values;

-- Single-line strings.
create table issue_string_values (
    issue_id bigint not null references issues (id) on delete cascade,
    field_id bigint not null,
    value text not null,
    primary key (issue_id, field_id)
);
create index issue_string_values_value_idx on issue_string_values (field_id, value, issue_id);
create statistics issue_string_values_field_value (mcv, dependencies, ndistinct)
    on field_id, value from issue_string_values;

-- Rich-text values (Markdown): they feed text search and have no ordering index.
create table issue_text_values (
    issue_id bigint not null references issues (id) on delete cascade,
    field_id bigint not null,
    value text not null,
    primary key (issue_id, field_id)
);

-- Change history of issues (HIST, architecture.md §3): append-only, written in the transaction of
-- the change. `removed` and `added` hold what left and what came, in a shape per kind (D-029).
create table issue_history (
    id bigint generated always as identity primary key,
    issue_id bigint not null references issues (id) on delete cascade,
    at timestamptz not null default now(),
    -- On whose behalf the change was made (HIST-012).
    author_id bigint not null references users (id),
    -- Where it came from: `web`, `api`, or `system` for background work.
    origin text not null check (origin in ('web', 'api', 'system')),
    -- Made without notifications (HIST-012).
    silent boolean not null default false,
    kind text not null check (kind in (
        'created', 'summary', 'description', 'field', 'project', 'tags', 'visibility',
        'resolved', 'reopened', 'deleted')),
    -- The field of a `field` record: viewers who may not read it do not see the record (HIST-007).
    field_id bigint,
    removed jsonb,
    added jsonb,
    check ((kind = 'field') = (field_id is not null))
);
create index issue_history_issue_idx on issue_history (issue_id, id);
create index issue_history_author_idx on issue_history (author_id);

-- Tags (ISS-034): personal to their owner until sharing arrives (ISS-037, step 1.5).
create table tags (
    id bigint generated always as identity primary key,
    name text not null,
    owner_id bigint not null references users (id),
    -- Color scheme: an index into the web app's palette (the same twelve as values); null for none.
    color smallint check (color between 0 and 11),
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create unique index tags_name_key on tags (owner_id, lower(name));

create table issue_tags (
    issue_id bigint not null references issues (id) on delete cascade,
    tag_id bigint not null references tags (id) on delete cascade,
    added_at timestamptz not null default now(),
    primary key (issue_id, tag_id)
);
create index issue_tags_tag_idx on issue_tags (tag_id, issue_id);

-- People watching an issue (ISS-030): the star. Why someone gets notifications is the business of
-- notifications (step 1.10).
create table issue_watchers (
    issue_id bigint not null references issues (id) on delete cascade,
    user_id bigint not null references users (id),
    created_at timestamptz not null default now(),
    primary key (issue_id, user_id)
);
create index issue_watchers_user_idx on issue_watchers (user_id, issue_id);

-- Votes (ISS-032): one per person and issue; `issues.votes` counts them.
create table issue_votes (
    issue_id bigint not null references issues (id) on delete cascade,
    user_id bigint not null references users (id),
    created_at timestamptz not null default now(),
    primary key (issue_id, user_id)
);
create index issue_votes_user_idx on issue_votes (user_id);
