-- Live events (step 1.6, D-040, architecture.md §5): the ordered log the gateways of all nodes
-- read. One row per change of one issue, comment, attachment or link, written in the change's
-- transaction under the publishing lock, so IDs grow in commit order. No foreign keys: it is a
-- log, purged after an hour.
create table live_events (
    id bigint generated always as identity primary key, -- taken under the publishing lock
    at timestamptz not null default now(),
    -- `issue`, `comment`, `attachment`, `issue_link`.
    kind text not null,
    entity_id bigint not null,
    -- After the change; for a deletion the last one.
    version bigint not null,
    change text not null check (change in ('created', 'updated', 'deleted')),
    -- Issues whose topics get the event: one, or both ends of a link.
    issues bigint[] not null default '{}',
    -- Projects for list topics: the issue's, or both for a move.
    projects bigint[] not null default '{}',
    -- Who may receive it (`domain::live::Guard`).
    guard jsonb not null
);
create index live_events_at_idx on live_events (at);
