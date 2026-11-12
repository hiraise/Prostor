-- Accounts and sign-in (step 0.5): users, passwords, sessions, personal API tokens and the state
-- of the first-run setup. Design: decisions.md D-023.

-- People and built-in accounts (ACC). Roles and groups arrive with step 1.1.
create table users (
    id bigint generated always as identity primary key,
    -- Sign-in name; ASCII only (domain rule), so lower() compares the same in every locale.
    username text not null,
    full_name text not null,
    -- Null for accounts without one, such as the guest.
    email text,
    -- Accounts the server itself needs: `admin`, made by the setup wizard and the target of
    -- recovery (ADM-051), and `guest`, which stands for anonymous visitors (ACC-001).
    builtin text unique check (builtin in ('admin', 'guest')),
    -- System administrator. Temporary: step 1.1 turns it into a role assignment.
    system_admin boolean not null default false,
    -- Set while the account is disabled: no sign-in, no tokens.
    disabled_at timestamptz,
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create unique index users_username_key on users (lower(username));
create unique index users_email_key on users (lower(email)) where email is not null;

-- Built-in passwords (AUTH-001). A separate table keeps hashes out of every query on users.
create table passwords (
    user_id bigint primary key references users (id) on delete cascade,
    -- argon2id in the PHC string format: algorithm, parameters, salt and hash.
    hash text not null,
    changed_at timestamptz not null default now()
);

-- Browser sessions after password sign-in (AUTH-003…AUTH-005). Expiry is computed from these
-- times and the current settings on every request (crates/domain/src/auth.rs).
create table sessions (
    id bigint generated always as identity primary key,
    user_id bigint not null references users (id) on delete cascade,
    -- SHA-256 of the cookie value; the value itself is never stored.
    token_hash bytea not null unique,
    -- Signed in with "remember me".
    remember boolean not null,
    created_at timestamptz not null,
    -- Last request, written at most once a minute.
    last_seen_at timestamptz not null,
    -- Where the sign-in came from, for the list of sessions (AUTH-020).
    ip inet,
    user_agent text
);
create index sessions_user_idx on sessions (user_id);

-- Personal API tokens (AUTH-029, AUTH-030).
create table api_tokens (
    id bigint generated always as identity primary key,
    user_id bigint not null references users (id) on delete cascade,
    name text not null,
    -- Scope keys such as `tracker.read` (crates/domain/src/auth.rs).
    scopes text[] not null,
    -- SHA-256 of the secret; the secret is shown once and never stored.
    token_hash bytea not null unique,
    -- Who created it: the owner, or an administrator for a bot account (AUTH-031).
    created_by bigint references users (id) on delete set null,
    created_at timestamptz not null default now(),
    -- Last use, written at most once a minute.
    last_used_at timestamptz,
    version bigint not null default 1
);
create index api_tokens_user_idx on api_tokens (user_id, id desc);
create index api_tokens_created_by_idx on api_tokens (created_by) where created_by is not null;

-- Facts about this installation; exactly one row once a node has started.
create table instance (
    id boolean primary key default true check (id),
    -- The code the setup wizard asks for (ADM-047), printed in the server log; cleared when the
    -- setup is complete.
    setup_code text,
    setup_completed_at timestamptz
);

-- Foreign keys promised by the foundation migration, now that users exist.
alter table settings
    add constraint settings_updated_by_fkey foreign key (updated_by) references users (id);
create index settings_updated_by_idx on settings (updated_by) where updated_by is not null;
alter table audit_events
    add constraint audit_events_actor_id_fkey foreign key (actor_id) references users (id);
alter table jobs
    add constraint jobs_initiator_id_fkey foreign key (initiator_id) references users (id);
