-- Deliveries (step 1.10, plan §3.4): pending event batches and the inbox. Written by every
-- change writer inside its transaction; fired by the notification sweep.

-- One user-visible change waiting to fire (plan §3.2). `updated` batches are held one minute
-- and merge (NTF-044); every other kind is due at once.
create table notification_pending (
    id bigint generated always as identity primary key,
    issue_id bigint not null references issues (id) on delete cascade,
    author_id bigint not null references users (id) on delete cascade,
    kind text not null,
    -- The change, one JSON line per fact: field diffs (name, old, new), comment ids, tag names,
    -- link and file lists (plan §3.4). A merged `updated` batch appends its lines.
    payload jsonb not null,
    -- Accounts mentioned by this change's new comments or description edits (NTF-035).
    mentions bigint[] not null default '{}',
    created_at timestamptz not null default now(),
    due_at timestamptz not null
);
create index notification_pending_due_idx on notification_pending (due_at);
create index notification_pending_issue_idx on notification_pending (issue_id, kind);

-- The inbox (plan §3.5): one row per recipient per fired batch, rendered for that recipient at
-- fire time. `issue_id` is empty only for `system` rows (job reports, ADM-031).
create table notifications (
    id bigint generated always as identity primary key,
    user_id bigint not null references users (id) on delete cascade,
    issue_id bigint references issues (id) on delete cascade,
    author_id bigint not null references users (id) on delete cascade,
    kind text not null,
    -- Why: `watch`, `mention`, `tag:<id>`, `search:<id>`; empty for `system` rows.
    reasons text[] not null default '{}',
    content jsonb not null,
    -- NTF-043: the final notification before access loss («наблюдение прекращено»).
    last boolean not null default false,
    read_at timestamptz,
    created_at timestamptz not null default now()
);
create index notifications_user_idx on notifications (user_id, created_at desc);
create index notifications_last_idx on notifications (user_id, issue_id) where last;
