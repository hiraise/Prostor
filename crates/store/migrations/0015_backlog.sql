-- The backlog's priority (step 1.8b, BRD-093): a manual order of a saved search. Ranked issues come
-- first by rank, the rest after them in the query's sort.
create table saved_search_ranks (
    search_id bigint not null references saved_searches (id) on delete cascade,
    issue_id bigint not null references issues (id) on delete cascade,
    rank text collate "C" not null,
    primary key (search_id, issue_id)
);
create index saved_search_ranks_rank_idx on saved_search_ranks (search_id, rank);
create index saved_search_ranks_issue_idx on saved_search_ranks (issue_id);
