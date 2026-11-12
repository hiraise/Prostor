-- Interface settings of a person that follow them across devices (UI-018, D-041). `data` holds
-- only keys that passed `domain::preferences`; an absent key means its default.
create table user_preferences (
    user_id bigint primary key references users (id) on delete cascade,
    data jsonb not null default '{}',
    updated_at timestamptz not null default now()
);

-- The sweep of empty drafts (ISS-010) reads drafts by their last change.
create index issues_drafts_updated_idx on issues (updated_at) where number is null;
