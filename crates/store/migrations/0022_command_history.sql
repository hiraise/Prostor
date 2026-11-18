-- The command history (step 2.1 §6, CMD-030): the lines a person applied, newest first, at most
-- 20 — the `recent` group of the command dialog's completion. One row per distinct line: a repeat
-- bumps `used_at`.

create table command_history (
    user_id bigint not null references users (id) on delete cascade,
    command text not null,
    used_at timestamptz not null default now(),
    primary key (user_id, command)
);

create index command_history_recent on command_history (user_id, used_at desc);
