-- Access (step 1.1): groups, roles, role assignments and capabilities; account states (invited,
-- disabled, deleted), verified emails, forced password changes and one-time links.
-- Design: decisions.md D-025 (access) and D-026 (account emails).

-- Accounts (ACC-004, ACC-005, ACC-006, ACC-013, ACC-017, AUTH-014).
alter table users
    -- When the current email was confirmed (AUTH-014); null until then and after a change.
    add column email_verified_at timestamptz,
    -- Background of the generated avatar, chosen at creation and kept (ACC-017): an index into
    -- the web app's palette of eight colors.
    add column avatar_color smallint not null default (floor(random() * 8))::smallint
        check (avatar_color between 0 and 7),
    -- Why the account was disabled, and the descriptive word shown with it (ACC-005).
    add column disabled_reason text,
    add column disabled_label text
        check (disabled_label in ('disabled', 'suspended', 'left', 'blocked')),
    -- Set while an invitation has not been accepted (ACC-004).
    add column invited_at timestamptz,
    -- A deleted account stays as a row without personal data, so whatever it authored keeps its
    -- reference (ACC-006); lookups by its ID resolve to the replacement (ACC-013).
    add column deleted_at timestamptz,
    add column replaced_by bigint references users (id);
create index users_replaced_by_idx on users (replaced_by) where replaced_by is not null;
-- Usernames of deleted accounts may be taken again.
drop index users_username_key;
create unique index users_username_key on users (lower(username)) where deleted_at is null;
-- The directory lists accounts by name (ACC-002).
create index users_directory_idx on users (lower(full_name), id) where deleted_at is null;

-- Set by an administrator: the next sign-in must choose a new password first (AUTH-010).
alter table passwords add column must_change boolean not null default false;

-- Key for pseudonyms of people a viewer may not see (ACC-016); filled by the first node.
alter table instance add column secret bytea;

-- Groups of accounts (ACC-021…ACC-032). Members of the built-in groups are implicit: Everyone is
-- every account, All members every account but the guest.
create table groups (
    id bigint generated always as identity primary key,
    name text not null,
    description text not null default '',
    -- `everyone` or `all_members`; their names are shown in the viewer's language.
    builtin text unique check (builtin in ('everyone', 'all_members')),
    -- Every new account joins the group (ACC-024).
    auto_join boolean not null default false,
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create unique index groups_name_key on groups (lower(name));
create index groups_auto_join_idx on groups (id) where auto_join and builtin is null;

-- Explicit memberships of the other groups.
create table group_members (
    group_id bigint not null references groups (id) on delete cascade,
    user_id bigint not null references users (id) on delete cascade,
    added_at timestamptz not null default now(),
    primary key (group_id, user_id)
);
create index group_members_user_idx on group_members (user_id, group_id);

-- IDs of deleted groups and the group that replaced each (ACC-013, ACC-032).
create table group_aliases (
    old_id bigint primary key,
    group_id bigint not null references groups (id) on delete cascade
);
create index group_aliases_group_idx on group_aliases (group_id);

-- Roles (ACC-052…ACC-057). Predefined roles take their permissions from code and cannot change;
-- custom roles keep theirs here.
create table roles (
    id bigint generated always as identity primary key,
    -- Key of a predefined role (crates/domain/src/access/role.rs); null for custom roles.
    builtin text unique,
    name text not null,
    description text not null default '',
    -- Permission keys of a custom role, closed under implication.
    permissions text[] not null default '{}',
    version bigint not null default 1,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (builtin is null or permissions = '{}')
);
create unique index roles_name_key on roles (lower(name));

-- Who holds which role where (ACC-058): a user or a group, globally or in one project.
create table role_assignments (
    id bigint generated always as identity primary key,
    role_id bigint not null references roles (id) on delete cascade,
    user_id bigint references users (id) on delete cascade,
    group_id bigint references groups (id) on delete cascade,
    -- Null means global scope. The foreign key to projects comes with projects (step 1.2).
    project_id bigint,
    created_at timestamptz not null default now(),
    created_by bigint references users (id) on delete set null,
    check ((user_id is null) <> (group_id is null)),
    unique nulls not distinct (role_id, user_id, group_id, project_id)
);
create index role_assignments_user_idx on role_assignments (user_id) where user_id is not null;
create index role_assignments_group_idx on role_assignments (group_id) where group_id is not null;
create index role_assignments_project_idx on role_assignments (project_id)
    where project_id is not null;
create index role_assignments_created_by_idx on role_assignments (created_by)
    where created_by is not null;

-- Groups for which a capability is on (ACC-064); no rows means off. Keys:
-- crates/domain/src/access/capability.rs.
create table capability_groups (
    capability text not null,
    group_id bigint not null references groups (id) on delete cascade,
    primary key (capability, group_id)
);
create index capability_groups_group_idx on capability_groups (group_id);

-- Emailed one-time links (AUTH-008, AUTH-014, ACC-004). Using a link deletes it.
create table account_links (
    id bigint generated always as identity primary key,
    user_id bigint not null references users (id) on delete cascade,
    purpose text not null check (purpose in ('password_reset', 'email_verification', 'invitation')),
    -- The address the link went to; a verification confirms exactly this address.
    email text not null,
    -- SHA-256 of the secret in the link; the secret itself is never stored.
    token_hash bytea not null unique,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);
create index account_links_user_idx on account_links (user_id, purpose);
create index account_links_expires_idx on account_links (expires_at);

-- Built-in groups, predefined roles and their default grants (ACC-021, ACC-052, ACC-066).
insert into groups (name, builtin, auto_join) values
    ('Everyone', 'everyone', true),
    ('All members', 'all_members', true);
insert into roles (name, builtin) values
    ('System administrator', 'system_admin'),
    ('Project administrator', 'project_admin'),
    ('Project member', 'project_member'),
    ('Directory viewer', 'directory_viewer'),
    ('Project creator', 'project_creator'),
    ('Account creator', 'account_creator');
insert into role_assignments (role_id, group_id)
select r.id, g.id from roles r, groups g
where r.builtin = 'directory_viewer' and g.builtin = 'all_members';
insert into capability_groups (capability, group_id)
select 'share_views', id from groups where builtin = 'everyone';

-- The administrator flag of step 0.5 becomes a role assignment.
insert into role_assignments (role_id, user_id)
select r.id, u.id from roles r, users u
where r.builtin = 'system_admin' and u.system_admin;
alter table users drop column system_admin;
