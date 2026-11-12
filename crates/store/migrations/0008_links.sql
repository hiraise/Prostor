-- Links between issues (step 1.4c): link types, links, subtasks. Design: decisions.md D-034.

-- Kinds of links (LNK-001). The source side name is how the source refers to the target, the
-- target side name how the target refers to the source; undirected types have one name.
-- Hierarchy types (subtasks, duplicates) give each target one source and allow no cycles.
create table link_types (
    id bigint generated always as identity primary key,
    -- What administrators call the type.
    name text not null,
    source_name text not null,
    target_name text,
    direction text not null check (direction in ('undirected', 'directed', 'hierarchy')),
    -- Key of a built-in type (crates/domain/src/links.rs); null for others.
    builtin text unique,
    -- Built-in types the server relies on: no edits, no deletion (LNK-002).
    locked boolean not null default false,
    -- Deleted types go to the trash (D-013): their links are hidden until a restore.
    deleted_at timestamptz,
    deleted_by bigint references users (id),
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check ((direction = 'undirected') = (target_name is null))
);
create unique index link_types_name_key on link_types (lower(name)) where deleted_at is null;
create index link_types_deleted_by_idx on link_types (deleted_by) where deleted_by is not null;

-- Links (LNK-003): one row per link, read from both issues.
create table issue_links (
    id bigint generated always as identity primary key,
    type_id bigint not null references link_types (id),
    source_id bigint not null references issues (id) on delete cascade,
    target_id bigint not null references issues (id) on delete cascade,
    created_at timestamptz not null default now(),
    created_by bigint not null references users (id),
    check (source_id <> target_id)
);
-- At most one link of a type between two issues, whichever the direction.
create unique index issue_links_pair_key
    on issue_links (type_id, least(source_id, target_id), greatest(source_id, target_id));
create index issue_links_source_idx on issue_links (source_id, type_id);
create index issue_links_target_idx on issue_links (target_id, type_id);
create index issue_links_type_idx on issue_links (type_id);
create index issue_links_created_by_idx on issue_links (created_by);

-- Built-in link types are created once, like the default fields.
alter table instance add column link_types_seeded_at timestamptz;

-- Link records in the history: `[{"type", "side", "issue"}]` removed and added, `side` being the
-- side of the issue the record belongs to.
alter table issue_history drop constraint issue_history_kind_check;
alter table issue_history add constraint issue_history_kind_check check (kind in (
    'created', 'summary', 'description', 'field', 'project', 'tags', 'visibility',
    'resolved', 'reopened', 'deleted',
    'comment', 'comment_edited', 'comment_deleted', 'comment_restored',
    'attachments', 'links'));
