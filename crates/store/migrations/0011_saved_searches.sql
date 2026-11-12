-- Sharing of tags and saved searches (step 1.5d, D-038): audiences of accounts and groups, the
-- saved searches with their stored queries, the predefined searches of each person.

-- Tags (ISS-037): who sees a tag on issues, who puts it on and takes it off, who renames it. The
-- three lists are independent; the owner is never in them.
alter table tags
    add column view_users bigint[] not null default '{}',
    add column view_groups bigint[] not null default '{}',
    add column use_users bigint[] not null default '{}',
    add column use_groups bigint[] not null default '{}',
    add column edit_users bigint[] not null default '{}',
    add column edit_groups bigint[] not null default '{}';

create index tags_view_users_idx on tags using gin (view_users) where cardinality(view_users) > 0;
create index tags_view_groups_idx on tags using gin (view_groups)
    where cardinality(view_groups) > 0;
create index tags_use_users_idx on tags using gin (use_users) where cardinality(use_users) > 0;
create index tags_use_groups_idx on tags using gin (use_groups) where cardinality(use_groups) > 0;
create index tags_edit_users_idx on tags using gin (edit_users) where cardinality(edit_users) > 0;
create index tags_edit_groups_idx on tags using gin (edit_groups)
    where cardinality(edit_groups) > 0;

-- Saved searches (QRY-063…QRY-068). `query` is the stored form: the parsed tree with each name
-- bound to an ID (domain::search::Stored), so renames never break it.
create table saved_searches (
    id bigint generated always as identity primary key,
    owner_id bigint not null references users (id),
    name text not null,
    query jsonb not null,
    -- Key of a predefined search (QRY-068); null for the person's own.
    builtin text,
    -- The "unassigned" search of a project (PRJ-019); goes with the project.
    project_id bigint references projects (id) on delete cascade,
    -- A deleted predefined search stays hidden, so it is not created again.
    hidden_at timestamptz,
    view_users bigint[] not null default '{}',
    view_groups bigint[] not null default '{}',
    edit_users bigint[] not null default '{}',
    edit_groups bigint[] not null default '{}',
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    version bigint not null default 1
);
create unique index saved_searches_name_key on saved_searches (owner_id, lower(name))
    where hidden_at is null;
create unique index saved_searches_builtin_key on saved_searches (owner_id, builtin)
    where builtin is not null;
create index saved_searches_project_idx on saved_searches (project_id)
    where project_id is not null;
create index saved_searches_view_users_idx on saved_searches using gin (view_users)
    where cardinality(view_users) > 0;
create index saved_searches_view_groups_idx on saved_searches using gin (view_groups)
    where cardinality(view_groups) > 0;
create index saved_searches_edit_users_idx on saved_searches using gin (edit_users)
    where cardinality(edit_users) > 0;
create index saved_searches_edit_groups_idx on saved_searches using gin (edit_groups)
    where cardinality(edit_groups) > 0;
