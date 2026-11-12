-- Rich text and comments (step 1.4a): rendered HTML stored with every Markdown text, comments
-- with their audiences, and comment records in the change history. Design: decisions.md D-031;
-- visibility architecture.md §4.

-- Comments on issues (CNT-001…CNT-005). Deleting one keeps it as a placeholder (CNT-005).
create table comments (
    id bigint generated always as identity primary key,
    issue_id bigint not null references issues (id) on delete cascade,
    author_id bigint not null references users (id),
    -- Markdown as written.
    text text not null,
    -- Sanitized HTML with markers (crate `markup`); viewer-dependent parts resolve when read.
    html text not null,
    -- What the text refers to: accounts mentioned (CNT-054), issue ID tokens (CNT-056) and file
    -- names it embeds (CNT-044).
    mentioned_users bigint[] not null default '{}',
    issue_tokens text[] not null default '{}',
    file_names text[] not null default '{}',
    -- `markup::RENDER_VERSION` the HTML was made with; older rows are rendered again by a job.
    render_version smallint not null,
    created_at timestamptz not null default now(),
    -- Null until the first edit.
    updated_at timestamptz,
    updated_by bigint references users (id),
    -- Whom the comment is limited to, inside the issue's audience (CNT-003). Both empty: the same
    -- people as the issue.
    audience_users bigint[] not null default '{}',
    audience_groups bigint[] not null default '{}',
    deleted_at timestamptz,
    deleted_by bigint references users (id),
    version bigint not null default 1
);
create index comments_issue_idx on comments (issue_id, id);
create index comments_author_idx on comments (author_id);
create index comments_updated_by_idx on comments (updated_by) where updated_by is not null;
create index comments_deleted_by_idx on comments (deleted_by) where deleted_by is not null;
-- Replacing deleted accounts and groups in audiences (architecture.md §6).
create index comments_audience_users_idx on comments using gin (audience_users)
    where cardinality(audience_users) > 0;
create index comments_audience_groups_idx on comments using gin (audience_groups)
    where cardinality(audience_groups) > 0;
-- Mention search (CNT-058, step 1.5) and replacing deleted accounts.
create index comments_mentioned_users_idx on comments using gin (mentioned_users)
    where cardinality(mentioned_users) > 0;

-- Rendered descriptions of issues. `render_version` 0: not rendered yet (rows from before this
-- step; the job renders them). No index on it: only that rare job filters by it, walking the
-- primary key.
alter table issues
    add column description_html text not null default '',
    add column mentioned_users bigint[] not null default '{}',
    add column issue_tokens text[] not null default '{}',
    add column file_names text[] not null default '{}',
    add column render_version smallint not null default 0;
create index issues_mentioned_users_idx on issues using gin (mentioned_users)
    where cardinality(mentioned_users) > 0;

-- Rendered values of text fields.
alter table issue_text_values
    add column html text not null default '',
    add column mentioned_users bigint[] not null default '{}',
    add column issue_tokens text[] not null default '{}',
    add column file_names text[] not null default '{}',
    add column render_version smallint not null default 0;

-- Rendered project descriptions (PRJ-006), in the basic profile.
alter table projects
    add column description_html text not null default '',
    add column render_version smallint not null default 0;

-- The rendering rules every stored text has been brought to; startup compares it with
-- `markup::RENDER_VERSION` and queues the re-rendering job.
alter table instance add column render_version smallint not null default 0;

-- Comment records in the history (HIST-006): posting is a stream item; edits, deletions and
-- restorations are kept for the comment's own history (CNT-008) and notifications (1.10).
alter table issue_history
    add column comment_id bigint references comments (id) on delete cascade;
create index issue_history_comment_idx on issue_history (comment_id) where comment_id is not null;
alter table issue_history drop constraint issue_history_kind_check;
alter table issue_history add constraint issue_history_kind_check check (kind in (
    'created', 'summary', 'description', 'field', 'project', 'tags', 'visibility',
    'resolved', 'reopened', 'deleted',
    'comment', 'comment_edited', 'comment_deleted', 'comment_restored'));
alter table issue_history add constraint issue_history_comment_check check (
    (kind in ('comment', 'comment_edited', 'comment_deleted', 'comment_restored'))
    = (comment_id is not null));
