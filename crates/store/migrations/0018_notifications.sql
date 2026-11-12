-- Notification settings and subscriptions (step 1.10, plan §3.1). Deliveries (pending batches,
-- the inbox) arrive with migration 0019 in 1.10a-2.

-- One row per person, written on their first change; no row = the defaults of
-- `domain::notifications` (NTF-017: the built-in watch subscription, all kinds on).
create table notification_settings (
    user_id bigint primary key references users (id) on delete cascade,
    -- The inbox channel is always on (NTF-001); this is the email channel and its form.
    email boolean not null default true,
    email_html boolean not null default true,     -- NTF-066
    own_changes boolean not null default false,   -- NTF-030
    mentions boolean not null default true,       -- NTF-034
    -- The kinds of the built-in watch subscription (NTF-017).
    watch_created boolean not null default true,
    watch_updated boolean not null default true,
    watch_resolved boolean not null default true,
    watch_comment boolean not null default true,
    watch_vote boolean not null default true,
    watch_tag boolean not null default true
);

-- Subscriptions on sources (NTF-021): a tag or a saved search, exactly one of the two. A deleted
-- account or source takes its rows with it: sources are not passed on to heirs (unlike the
-- sources themselves, D-038).
create table notification_subscriptions (
    id bigint generated always as identity primary key,
    user_id bigint not null references users (id) on delete cascade,
    tag_id bigint references tags (id) on delete cascade,
    saved_search_id bigint references saved_searches (id) on delete cascade,
    kinds text[] not null,
    created_at timestamptz not null default now(),
    unique (user_id, tag_id),
    unique (user_id, saved_search_id),
    check ((tag_id is null) <> (saved_search_id is null))
);
create index notification_subscriptions_tag_idx on notification_subscriptions (tag_id);
create index notification_subscriptions_saved_search_idx
    on notification_subscriptions (saved_search_id);
