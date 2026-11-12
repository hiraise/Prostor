-- Queries (step 1.5b, D-036): indexes for the default order of issue lists, `updated desc`, across
-- projects and within one, and for `resolved:` ranges. Other sorts and filters use the indexes of
-- 0005 (`id` follows publication closely enough for `created`); and people by full name.

create index issues_updated_idx on issues (updated_at desc, id desc)
    where number is not null and deleted_at is null;
create index issues_project_updated_idx on issues (project_id, updated_at desc, id desc)
    where number is not null and deleted_at is null;
create index issues_resolved_at_idx on issues (resolved_at)
    where resolved_at is not null and number is not null and deleted_at is null;

-- People in queries are found by login or by folded full name (`store::search::catalog::users`):
-- the same expression as there.
create index users_folded_name_idx
    on users (btrim(regexp_replace(replace(lower(full_name), 'ё', 'е'), '[[:space:]_-]+', ' ', 'g')))
    where deleted_at is null;
