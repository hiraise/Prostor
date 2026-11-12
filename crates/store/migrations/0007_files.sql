-- Files (step 1.4b): staged uploads, attachments of issues and comments, avatars and project
-- logos. The files themselves live in the file store (crate `files`); rows keep their keys.
-- Design: decisions.md D-033; visibility architecture.md §4.

-- Files uploaded and not attached yet (CNT-017, CNT-034). A job removes each a day later.
create table uploads (
    id bigint generated always as identity primary key,
    owner_id bigint not null references users (id) on delete cascade,
    storage_key text not null,
    name text not null,
    size bigint not null,
    -- What the server recognized from the bytes, never what the client said.
    content_type text not null,
    sha256 bytea not null,
    created_at timestamptz not null default now()
);
create index uploads_owner_idx on uploads (owner_id, id);

-- Files of issues (CNT-017), some of them posted with a comment (CNT-018). Deleting one hides it
-- at once; the row and the file stay until the trash purge (D-013).
create table attachments (
    id bigint generated always as identity primary key,
    issue_id bigint not null references issues (id) on delete cascade,
    comment_id bigint references comments (id) on delete cascade,
    author_id bigint not null references users (id),
    name text not null,
    size bigint not null,
    content_type text not null,
    sha256 bytea not null,
    storage_key text not null,
    -- Pictures (CNT-022): filled by a job after the upload.
    width integer,
    height integer,
    thumbnail_key text,
    -- Part of every link's signature: replacing it would revoke the file's links.
    secret bytea not null,
    -- Whom the file is limited to, inside the issue's audience (ACC-049). Both empty: the same
    -- people as the issue (or the comment, for a comment's file).
    audience_users bigint[] not null default '{}',
    audience_groups bigint[] not null default '{}',
    created_at timestamptz not null default now(),
    updated_at timestamptz,
    updated_by bigint references users (id),
    deleted_at timestamptz,
    deleted_by bigint references users (id),
    version bigint not null default 1
);
create index attachments_issue_idx on attachments (issue_id, id) where deleted_at is null;
create index attachments_comment_idx on attachments (comment_id) where comment_id is not null;
create index attachments_author_idx on attachments (author_id);
create index attachments_updated_by_idx on attachments (updated_by) where updated_by is not null;
create index attachments_deleted_by_idx on attachments (deleted_by) where deleted_by is not null;
-- Replacing deleted accounts and groups in audiences (architecture.md §6).
create index attachments_audience_users_idx on attachments using gin (audience_users)
    where cardinality(audience_users) > 0;
create index attachments_audience_groups_idx on attachments using gin (audience_groups)
    where cardinality(audience_groups) > 0;

-- Uploaded avatars (ACC-017) and each person's link secret: file links carry who made them, and
-- a new secret revokes all of them (CNT-026).
alter table users
    add column avatar_key text,
    add column file_link_secret bytea;

-- Uploaded project logos (PRJ-007).
alter table projects add column logo_key text;

-- Attachment records in the history: `[{"id", "name"}]` removed and added.
alter table issue_history drop constraint issue_history_kind_check;
alter table issue_history add constraint issue_history_kind_check check (kind in (
    'created', 'summary', 'description', 'field', 'project', 'tags', 'visibility',
    'resolved', 'reopened', 'deleted',
    'comment', 'comment_edited', 'comment_deleted', 'comment_restored',
    'attachments'));
