-- Projects and fields (step 1.2): projects with their keys, teams and favorites; custom fields,
-- value sets and their values; fields attached to projects. Design: decisions.md D-027 (projects)
-- and D-028 (fields). Values of fields on issues arrive with issues (step 1.3, D-022).

-- Projects (PRJ-001…PRJ-065).
create table projects (
    id bigint generated always as identity primary key,
    -- Current key: the prefix of issue IDs (PRJ-003). Every key the project ever had is also in
    -- project_keys, which keeps them unique across projects.
    key text not null unique,
    name text not null,
    -- Markdown (PRJ-006).
    description text not null default '',
    -- `standard`; `service_desk` comes with the helpdesk (P4). Fixed at creation (PRJ-001).
    kind text not null default 'standard' check (kind in ('standard', 'service_desk')),
    -- The built-in template the project was created from (PRJ-021).
    template text not null check (template in ('blank', 'scrum', 'kanban')),
    owner_id bigint not null references users (id),
    -- Background of the generated logo: an index into the web app's palette of eight (PRJ-007).
    logo_color smallint not null default (floor(random() * 8))::smallint
        check (logo_color between 0 and 7),
    -- The number the next published issue gets (PRJ-004). Numbers are never reused.
    next_number bigint not null default 1 check (next_number >= 1),
    -- Audience group or team pre-filled on new issues and articles (PRJ-039); null for none.
    default_audience_group_id bigint references groups (id) on delete set null,
    -- Archived projects are read-only and left out of pickers and searches (PRJ-061).
    archived_at timestamptz,
    -- Set while the project is deleted in the background (PRJ-065); nobody can use it then.
    deleting_at timestamptz,
    -- The job deleting it, for its progress and a retry.
    deletion_job_id bigint references jobs (id) on delete set null,
    created_by bigint references users (id),
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create unique index projects_name_key on projects (lower(name));
create index projects_owner_idx on projects (owner_id);
create index projects_created_by_idx on projects (created_by) where created_by is not null;
create index projects_audience_idx on projects (default_audience_group_id)
    where default_audience_group_id is not null;
create index projects_deletion_job_idx on projects (deletion_job_id)
    where deletion_job_id is not null;

-- Every key a project has had, the current one included (PRJ-005). Former keys stay reserved for
-- their project, so old issue IDs and links keep leading to it.
create table project_keys (
    key text primary key,
    project_id bigint not null references projects (id) on delete cascade,
    created_at timestamptz not null default now()
);
create index project_keys_project_idx on project_keys (project_id);

-- Projects a person starred (PRJ-012).
create table project_favorites (
    user_id bigint not null references users (id) on delete cascade,
    project_id bigint not null references projects (id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (user_id, project_id)
);
create index project_favorites_project_idx on project_favorites (project_id);

-- Project teams (PRJ-047, ACC-033) are groups that belong to a project: their users are explicit
-- members (group_members), their groups are in team_groups. A team's name follows its project's.
alter table groups
    add column team_of bigint unique references projects (id) on delete cascade;
drop index groups_name_key;
create unique index groups_name_key on groups (lower(name)) where team_of is null;

-- Groups in a project team: all their current members are in the team (PRJ-047).
create table team_groups (
    team_id bigint not null references groups (id) on delete cascade,
    group_id bigint not null references groups (id) on delete cascade,
    added_at timestamptz not null default now(),
    primary key (team_id, group_id)
);
create index team_groups_group_idx on team_groups (group_id);

-- How many accounts a team has: its users and the current members of its groups, the built-in
-- ones included (PRJ-046).
create function team_size(team bigint) returns bigint language sql stable as $$
    select case
        when exists (select 1 from team_groups t join groups g on g.id = t.group_id
                     where t.team_id = team and g.builtin = 'everyone')
            then (select count(*) from users where deleted_at is null)
        else (select count(*) from (
                select m.user_id from group_members m where m.group_id = team
                union
                select m.user_id from team_groups t join group_members m on m.group_id = t.group_id
                where t.team_id = team
                union
                select u.id from users u
                where u.deleted_at is null and u.builtin is distinct from 'guest'
                  and exists (select 1 from team_groups t join groups g on g.id = t.group_id
                              where t.team_id = team and g.builtin = 'all_members')
              ) members)
    end
$$;

-- The foreign key promised by the access migration, now that projects exist.
alter table role_assignments
    add constraint role_assignments_project_id_fkey foreign key (project_id)
        references projects (id) on delete cascade;

-- Value sets (FLD-029): named lists of allowed values that fields and projects share or own.
create table value_sets (
    id bigint generated always as identity primary key,
    name text not null,
    -- Which values it holds: enum, state, user, version, build or owned (values with an owner).
    kind text not null check (kind in ('enum', 'state', 'user', 'version', 'build', 'owned')),
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create unique index value_sets_name_key on value_sets (kind, lower(name));

-- Values of the sets of every kind but user (FLD-043…FLD-049). Properties that a kind does not
-- have stay at their defaults.
create table field_values (
    id bigint generated always as identity primary key,
    set_id bigint not null references value_sets (id) on delete cascade,
    name text not null,
    description text not null default '',
    -- Color scheme: an index into the web app's palette of value colors (FLD-044); null for none.
    color smallint check (color between 0 and 15),
    -- Manual order inside the set: a fractional rank (architecture.md §6, FLD-037).
    rank text collate "C" not null,
    -- Hidden from pickers, kept on issues (FLD-045).
    archived boolean not null default false,
    -- State values: an issue holding it counts as resolved (FLD-046).
    resolved boolean not null default false,
    -- Version values (FLD-047).
    released boolean not null default false,
    release_date date,
    start_date date,
    -- Build values (FLD-048).
    assembled_on date,
    -- Values with an owner (FLD-049).
    owner_id bigint references users (id),
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create unique index field_values_name_key on field_values (set_id, lower(name));
create index field_values_rank_idx on field_values (set_id, rank);
create index field_values_owner_idx on field_values (owner_id) where owner_id is not null;

-- Members of user sets (FLD-040): accounts, and groups whose current members all count.
create table value_set_users (
    set_id bigint not null references value_sets (id) on delete cascade,
    user_id bigint not null references users (id) on delete cascade,
    primary key (set_id, user_id)
);
create index value_set_users_user_idx on value_set_users (user_id);

create table value_set_groups (
    set_id bigint not null references value_sets (id) on delete cascade,
    group_id bigint not null references groups (id) on delete cascade,
    primary key (set_id, group_id)
);
create index value_set_groups_group_idx on value_set_groups (group_id);

-- Custom fields, defined once for the instance (FLD-015…FLD-023).
create table fields (
    id bigint generated always as identity primary key,
    name text not null,
    kind text not null check (kind in (
        'string', 'text', 'integer', 'float', 'date', 'date_time', 'duration',
        'enum', 'state', 'user', 'group', 'version', 'build', 'owned')),
    -- Several values per issue (FLD-001); part of the field's identity.
    multi boolean not null default false,
    -- Other words for the field in searches and commands (FLD-017).
    aliases text[] not null default '{}',
    -- Key of a field of the default set (crates/domain/src/fields/builtin.rs); null for others.
    builtin text unique,
    -- Attached to every project created from the blank template (FLD-020).
    auto_attach boolean not null default false,
    -- Instance-wide order, the initial order of new projects (FLD-022): a fractional rank.
    rank text collate "C" not null,
    -- The set new attachments share or copy (FLD-015, FLD-053); null for kinds without sets.
    set_id bigint references value_sets (id),
    -- Settings a new attachment starts with (FLD-019). `default_values` holds value IDs, or
    -- account IDs for user fields.
    can_be_empty boolean not null default true,
    empty_label text not null default '',
    private boolean not null default false,
    default_values bigint[] not null default '{}',
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (not multi or kind in ('enum', 'user', 'group', 'version', 'build', 'owned')),
    check ((set_id is not null) = (kind in ('enum', 'state', 'user', 'version', 'build', 'owned')))
);
-- One field per name and type (FLD-016).
create unique index fields_name_key on fields (lower(name), kind, multi);
create index fields_rank_idx on fields (rank);
create index fields_set_idx on fields (set_id) where set_id is not null;

-- Fields attached to projects with their per-project settings (FLD-051…FLD-060).
create table project_fields (
    project_id bigint not null references projects (id) on delete cascade,
    field_id bigint not null references fields (id) on delete cascade,
    -- The set this project uses; shared when other projects or fields use it too (FLD-030).
    set_id bigint references value_sets (id),
    -- Order in this project (FLD-058): a fractional rank.
    rank text collate "C" not null,
    can_be_empty boolean not null,
    -- Shown for an empty value and usable in searches (FLD-056).
    empty_label text not null default '',
    -- Value IDs, or account IDs for user fields (FLD-055).
    default_values bigint[] not null default '{}',
    -- Needs the private-field permissions to see or change (FLD-060).
    private boolean not null default false,
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (project_id, field_id)
);
create index project_fields_field_idx on project_fields (field_id);
create index project_fields_set_idx on project_fields (set_id) where set_id is not null;
create index project_fields_rank_idx on project_fields (project_id, rank);

-- When the default field set was created in the instance language (FLD-070); null until then.
alter table instance add column fields_seeded_at timestamptz;
