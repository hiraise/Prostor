-- Auto-watch switches (ISS-030, plan step 1.12 §7.1): what makes their owner a watcher. One
-- alter table, as the plan agrees; no row = every switch on, like the settings around them.

alter table notification_settings
    add column autowatch_created boolean not null default true,
    add column autowatch_commented boolean not null default true,
    add column autowatch_assigned boolean not null default true,
    add column autowatch_voted boolean not null default true;
