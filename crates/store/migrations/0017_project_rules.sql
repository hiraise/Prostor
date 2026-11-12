-- Built-in behaviors as per-project toggles (AUT-022, AUT-023, step 1.9). A rule with no row is
-- off: rows are written by project creation (the template's set) and by the settings tab only.
create table project_rules (
    project_id bigint not null references projects (id) on delete cascade,
    rule text not null,
    enabled boolean not null default true,
    primary key (project_id, rule)
);

-- Changes made by a behavior keep their triggering author and say so (plan step 1.9 §3.12).
alter table issue_history drop constraint issue_history_origin_check;
alter table issue_history add constraint issue_history_origin_check
    check (origin in ('web', 'api', 'system', 'rule'));

-- Existing projects get their template's set (AUT-022); the values are in domain::rules.
-- Blank and kanban share one set (PRJ-022, PRJ-024); the kanban stage rule arrives in P2.
insert into project_rules (project_id, rule)
select id, 'assignee_visibility' from projects where template in ('blank', 'scrum', 'kanban');
insert into project_rules (project_id, rule)
select id, 'clone_issue' from projects where template in ('blank', 'scrum', 'kanban');
insert into project_rules (project_id, rule)
select id, 'duplicates' from projects where template in ('blank', 'scrum', 'kanban');
insert into project_rules (project_id, rule)
select id, 'plus_one_vote' from projects where template in ('blank', 'scrum', 'kanban');
insert into project_rules (project_id, rule)
select id, 'component_owner' from projects where template in ('blank', 'kanban');
insert into project_rules (project_id, rule)
select id, 'inherit_fix_versions' from projects where template in ('blank', 'kanban');
insert into project_rules (project_id, rule)
select id, 'subtasks_follow' from projects where template in ('blank', 'kanban');
insert into project_rules (project_id, rule)
select id, 'fixed_in_build' from projects where template in ('blank', 'kanban');
insert into project_rules (project_id, rule)
select id, 'scrum' from projects where template = 'scrum';
