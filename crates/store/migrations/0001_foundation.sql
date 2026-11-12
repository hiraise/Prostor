-- Foundation of the schema (step 0.3): settings, audit events, background jobs.
-- Rules for migrations: conventions.md §6.

-- Instance-wide settings that differ from their defaults (ADM-002). Keys, types and defaults
-- are declared in code (crates/domain/src/settings.rs); a missing row means "default".
create table settings (
    key text primary key,
    value jsonb not null,
    updated_at timestamptz not null default now(),
    -- User who made the last change; null means the server itself. The foreign key to users
    -- is added together with the users table.
    updated_by bigint
);

-- Administrative changes, recorded from the first release (architecture.md §9.17, ADM-040).
create table audit_events (
    id bigint generated always as identity primary key,
    at timestamptz not null default now(),
    -- Null means the server itself.
    actor_id bigint,
    -- What happened, such as `setting.changed`.
    action text not null,
    -- What it happened to, such as `setting:base_url`.
    target text not null,
    -- Action-specific data, such as old and new values.
    details jsonb not null default '{}'
);
create index audit_events_target_idx on audit_events (target, id desc);
create index audit_events_actor_idx on audit_events (actor_id, id desc) where actor_id is not null;

-- Durable background jobs (ADM-031). How the queue works: architecture.md → Background jobs.
create table jobs (
    id bigint generated always as identity primary key,
    -- Handler name, such as `projects.delete`.
    kind text not null,
    payload jsonb not null default '{}',
    state text not null default 'queued'
        check (state in ('queued', 'running', 'finished', 'failed', 'cancelled')),
    -- User who started the job; null means the server itself.
    initiator_id bigint,
    -- At most one queued or running job per key, such as one index rebuild at a time.
    unique_key text,
    cancellable boolean not null default false,
    cancel_requested boolean not null default false,
    priority smallint not null default 0,
    -- Runs that count towards max_attempts; a run interrupted by shutdown does not count.
    attempts integer not null default 0,
    -- Incremented on every claim and never decreased. Every update by a worker is guarded by it,
    -- so a worker that lost its claim cannot overwrite the result of the next one.
    claim_token bigint not null default 0,
    max_attempts integer not null default 5 check (max_attempts >= 1),
    -- Not before this time: delayed jobs and retries with backoff.
    run_at timestamptz not null default now(),
    -- The worker running the job and until when its lease holds; renewed while it runs.
    worker text,
    lease_until timestamptz,
    progress_done bigint not null default 0,
    progress_total bigint,
    -- Latest status line from the handler.
    status_text text,
    last_error text,
    created_at timestamptz not null default now(),
    started_at timestamptz,
    finished_at timestamptz,
    updated_at timestamptz not null default now()
);
-- Claiming the next job.
create index jobs_queue_idx on jobs (priority desc, run_at, id) where state = 'queued';
-- Finding running jobs whose worker stopped renewing the lease.
create index jobs_lease_idx on jobs (lease_until) where state = 'running';
create unique index jobs_unique_key_idx on jobs (unique_key) where state in ('queued', 'running');
-- "My jobs" lists and counts per state.
create index jobs_initiator_idx on jobs (initiator_id, id desc) where initiator_id is not null;
create index jobs_state_idx on jobs (state, id desc);
