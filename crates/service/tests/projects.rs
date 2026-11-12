//! Projects on a real database (step 1.2): templates and the default field set, keys, visibility,
//! the team and its roles, ownership, archive and deletion, and what deleting accounts and groups
//! does to projects.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use time::OffsetDateTime;
use time::macros::datetime;
use trackflow_domain::access::Access;
use trackflow_domain::access::permission::{Permission, PermissionSet};
use trackflow_domain::access::role::PredefinedRole;
use trackflow_domain::projects::Template;
use trackflow_domain::settings::Language;
use trackflow_domain::validation::Problem;
use trackflow_domain::{ProjectId, RoleId, UserId};
use trackflow_service::access;
use trackflow_service::auth::SignedIn;
use trackflow_service::groups::{self, NewGroup};
use trackflow_service::projects::{self, NewProjectInput, ProjectChange};
use trackflow_service::roles::{self, NewRole};
use trackflow_service::setup::{self, Setup};
use trackflow_service::users::{self, NewAccount};
use trackflow_service::{Error, Passwords, project_fields, teams};
use trackflow_store::access::Holder;
use trackflow_store::projects::Section;
use trackflow_store::sessions::Origin;
use trackflow_store::{PgPool, value_sets};
use trackflow_testkit::TestDb;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);

fn passwords() -> Passwords {
    Passwords::new(4)
}

async fn set_up(pool: &PgPool, language: &str) -> SignedIn {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    setup::run(
        pool,
        &passwords(),
        Setup {
            code: "TEST-CODE-2345".to_owned(),
            username: "admin".to_owned(),
            full_name: "Anna Admin".to_owned(),
            email: "anna@example.com".to_owned(),
            password: "correct horse".to_owned(),
            system_language: language.to_owned(),
            base_url: None,
            time_zone: "UTC".to_owned(),
        },
        T0,
        &Origin::default(),
    )
    .await
    .unwrap()
}

async fn access_of(pool: &PgPool, user: UserId) -> Access {
    access::load(pool, user).await.unwrap()
}

async fn create_user(pool: &PgPool, actor: &Access, username: &str) -> UserId {
    users::create(
        pool,
        &passwords(),
        actor,
        NewAccount {
            username: username.to_owned(),
            full_name: format!("{username} Person"),
            email: format!("{username}@example.com"),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id
}

async fn role_id(pool: &PgPool, role: PredefinedRole) -> RoleId {
    trackflow_store::roles::builtin_id(pool, role.key())
        .await
        .unwrap()
        .unwrap()
}

fn input(template: Template, name: &str, key: Option<&str>) -> NewProjectInput {
    NewProjectInput {
        template,
        name: name.to_owned(),
        key: key.map(str::to_owned),
        first_number: None,
        description: String::new(),
    }
}

async fn create_project(
    pool: &PgPool,
    actor: &Access,
    template: Template,
    name: &str,
    key: &str,
) -> ProjectId {
    projects::create(pool, actor, input(template, name, Some(key)), Language::En)
        .await
        .unwrap()
        .project
        .id
}

fn problems(error: Error) -> Vec<(String, Problem)> {
    match error {
        Error::Invalid(problems) => problems.into_iter().map(|p| (p.path, p.problem)).collect(),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

/// Names of a project's fields in its order.
async fn field_names(pool: &PgPool, actor: &Access, project: ProjectId) -> Vec<String> {
    project_fields::list(pool, actor, project)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.field.name)
        .collect()
}

#[tokio::test]
async fn fld_070_setup_creates_the_default_field_set_in_the_instance_language() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "ru").await.user;
    let access = access_of(pool, admin.id).await;
    let all = trackflow_service::fields::list(pool, &access, "", None, None, 100)
        .await
        .unwrap();
    let names: Vec<&str> = all.iter().map(|row| row.field.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Приоритет",
            "Тип",
            "Состояние",
            "Исполнитель",
            "Компонент",
            "Исправлено в версиях",
            "Затронутые версии",
            "Исправлено в сборке",
            "Срок",
            "Оценка",
            "Затрачено"
        ]
    );
    let priority = &all[0];
    assert!(!priority.field.can_be_empty);
    assert_eq!(priority.default_values[0].name, "Обычный");
    assert_eq!(all.iter().filter(|row| row.field.auto_attach).count(), 10);
    // Fix and affected versions share the set of versions.
    assert_eq!(all[5].field.set_id, all[6].field.set_id);
    // Startup on an instance that has it already changes nothing.
    assert!(
        !trackflow_service::seed::ensure_default_fields(pool, Language::En)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn prj_019_prj_022_a_blank_project_gets_its_team_roles_and_the_default_fields() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let row = projects::create(
        pool,
        &access,
        NewProjectInput {
            description: "Our website".to_owned(),
            first_number: Some(100),
            ..input(Template::Blank, "Website redesign", None)
        },
        Language::En,
    )
    .await
    .unwrap();
    // PRJ-003, PRJ-018: the key comes from the name; PRJ-004: the first number.
    assert_eq!(row.project.key, "WR");
    assert_eq!(row.project.next_number, 100);
    assert_eq!(row.project.template, "blank");
    // PRJ-008: the creator owns it; PRJ-047: the team is named after it and holds the creator.
    assert_eq!(row.project.owner_id, admin.id);
    let team = row.team.as_ref().unwrap();
    assert_eq!(team.name, "Website redesign");
    assert_eq!(team.team_of, Some(row.project.id));
    assert_eq!(row.team_size, 1);
    let id = row.project.id;
    let people = teams::people(pool, &access, id).await.unwrap();
    let mut roles: Vec<(String, bool)> = people
        .assignments
        .iter()
        .map(|d| (d.role.builtin.clone().unwrap(), d.group.is_some()))
        .collect();
    roles.sort();
    // PRJ-019: project administrator directly, project member for the team.
    assert_eq!(
        roles,
        [
            ("project_admin".to_owned(), false),
            ("project_member".to_owned(), true)
        ]
    );
    // PRJ-022, FLD-020: every field marked for new projects, in the instance order.
    assert_eq!(
        field_names(pool, &access, id).await,
        [
            "Priority",
            "Type",
            "State",
            "Assignee",
            "Component",
            "Fix versions",
            "Affected versions",
            "Fixed in build",
            "Estimation",
            "Spent time"
        ]
    );
    let fields = project_fields::list(pool, &access, id).await.unwrap();
    // Priorities are shared; versions belong to the project, one list for both version fields.
    let global = trackflow_service::fields::list(pool, &access, "", None, None, 100)
        .await
        .unwrap();
    assert_eq!(fields[0].attached.set_id, global[0].field.set_id);
    assert_eq!(fields[0].default_values[0].name, "Normal");
    assert!(fields[0].shared_with == 0 && fields[0].field_default);
    assert_eq!(fields[5].attached.set_id, fields[6].attached.set_id);
    assert_ne!(fields[5].attached.set_id, global[5].field.set_id);
    assert_eq!(
        fields[5].set.as_ref().unwrap().name,
        "Website redesign: Versions"
    );
    // PRJ-054: the assignee set holds the team itself.
    let assignees = value_sets::get(pool, fields[3].attached.set_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let members = value_sets::members(&mut conn, &[assignees.id])
        .await
        .unwrap();
    assert_eq!(members.groups, [(assignees.id, team.id)]);
    // FLD-057: priority cannot be empty but has a default, so it is not required.
    assert!(!fields[0].required);
}

#[tokio::test]
async fn prj_023_prj_024_scrum_and_kanban_templates_bring_their_fields() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let scrum = create_project(pool, &access, Template::Scrum, "Mobile app", "MOB").await;
    assert_eq!(
        field_names(pool, &access, scrum).await,
        [
            "Priority",
            "Type",
            "State",
            "Assignee",
            "Sprints",
            "Story points",
            "Ideal days",
            "Estimation"
        ]
    );
    let fields = project_fields::list(pool, &access, scrum).await.unwrap();
    // The scrum types are the project's own set with Task by default.
    let types =
        trackflow_service::value_sets::get(pool, &access, fields[1].attached.set_id.unwrap())
            .await
            .unwrap();
    let names: Vec<&str> = types.values.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["Epic", "User story", "Task", "Bug"]);
    assert_eq!(fields[1].default_values[0].name, "Task");
    let states =
        trackflow_service::value_sets::get(pool, &access, fields[2].attached.set_id.unwrap())
            .await
            .unwrap();
    assert_eq!(states.values.iter().filter(|v| v.resolved).count(), 2);
    // A second scrum project reuses the Sprints field with a set of its own.
    let other = create_project(pool, &access, Template::Scrum, "Backend", "BE").await;
    let other_fields = project_fields::list(pool, &access, other).await.unwrap();
    assert_eq!(other_fields[4].field.id, fields[4].field.id);
    assert_ne!(other_fields[4].attached.set_id, fields[4].attached.set_id);

    let kanban = create_project(pool, &access, Template::Kanban, "Support flow", "SUP").await;
    let kanban_fields = project_fields::list(pool, &access, kanban).await.unwrap();
    assert_eq!(
        kanban_fields
            .iter()
            .map(|r| r.field.name.as_str())
            .collect::<Vec<_>>(),
        ["Stage", "Readiness", "Priority", "Assignee"]
    );
    assert_eq!(kanban_fields[0].field.kind, "state");
    assert_eq!(kanban_fields[0].default_values[0].name, "Backlog");
}

#[tokio::test]
async fn prj_002_prj_003_prj_005_prj_018_names_and_keys() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let id = create_project(pool, &access, Template::Blank, "Website", "WEB").await;
    // PRJ-018: the wizard learns about taken names and keys before creating.
    let check = projects::check(pool, &access, Some("website"), Some("web"))
        .await
        .unwrap();
    assert_eq!(check.suggested_key.as_deref(), Some("WEBS"));
    let found: Vec<(&str, Problem)> = check
        .problems
        .iter()
        .map(|p| (p.path.as_str(), p.problem))
        .collect();
    assert_eq!(found, [("name", Problem::Taken), ("key", Problem::Taken)]);
    let error = projects::create(
        pool,
        &access,
        input(Template::Blank, "Other", Some("web")),
        Language::En,
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("key".to_owned(), Problem::Taken)]);
    let error = projects::create(
        pool,
        &access,
        input(Template::Blank, " WEBSITE ", Some("WS")),
        Language::En,
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("name".to_owned(), Problem::Taken)]);

    // PRJ-005: a new key keeps the old one for this project.
    let row = projects::get(pool, &access, id).await.unwrap();
    let row = projects::update(
        pool,
        &access,
        id,
        ProjectChange {
            key: Some("site".to_owned()),
            name: Some("Company site".to_owned()),
            version: row.project.version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(row.project.key, "SITE");
    assert_eq!(row.former_keys, ["WEB"]);
    assert_eq!(row.team.unwrap().name, "Company site");
    let by_old = projects::by_key(pool, &access, "web").await.unwrap();
    assert_eq!(by_old.project.id, id);
    // Nobody else takes the former key; the project itself may go back to it.
    let other = create_project(pool, &access, Template::Blank, "Shop", "SHOP").await;
    let shop = projects::get(pool, &access, other).await.unwrap();
    let error = projects::update(
        pool,
        &access,
        other,
        ProjectChange {
            key: Some("WEB".to_owned()),
            version: shop.project.version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("key".to_owned(), Problem::Taken)]);
    let row = projects::update(
        pool,
        &access,
        id,
        ProjectChange {
            key: Some("WEB".to_owned()),
            version: row.project.version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(row.project.key, "WEB");
    assert_eq!(row.former_keys, ["SITE"]);
    // A stale version is refused.
    let error = projects::update(
        pool,
        &access,
        id,
        ProjectChange {
            description: Some("x".to_owned()),
            version: 1,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Conflict(_)));
}

#[tokio::test]
async fn prj_009_prj_010_prj_048_prj_056_projects_are_private_to_their_people() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let id = create_project(pool, &access, Template::Blank, "Secret", "SEC").await;
    let bob = create_user(pool, &access, "bob").await;
    let bob_access = access_of(pool, bob).await;
    // PRJ-056: nobody outside the team and role holders sees it.
    assert!(matches!(
        projects::get(pool, &bob_access, id).await.unwrap_err(),
        Error::NotFound(_)
    ));
    assert!(
        projects::list(pool, &bob_access, Section::Active, "", None, 50)
            .await
            .unwrap()
            .is_empty()
    );
    // PRJ-048: joining the team gives the team roles.
    let added = teams::add(pool, &access, id, &[bob], &[]).await.unwrap();
    assert_eq!(added.users, [bob]);
    let bob_access = access_of(pool, bob).await;
    assert!(bob_access.has_in(Permission::IssueCreate, id));
    assert!(!bob_access.has_in(Permission::ProjectUpdate, id));
    let list = projects::list(pool, &bob_access, Section::Active, "", None, 50)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    // ACC-061: the role comes through the team.
    let bob_user = trackflow_store::users::get(pool, bob)
        .await
        .unwrap()
        .unwrap();
    let effective = access::effective_roles(pool, &bob_user).await.unwrap();
    assert!(effective.iter().any(|role| role.scope
        == trackflow_domain::access::Scope::Project(id)
        && matches!(&role.sources[..], [access::Source::Group { .. }])));
    // A member cannot change the team or see the people page.
    assert!(matches!(
        teams::add(pool, &bob_access, id, &[admin.id], &[])
            .await
            .unwrap_err(),
        Error::Forbidden(_)
    ));
    assert!(matches!(
        teams::people(pool, &bob_access, id).await.unwrap_err(),
        Error::Forbidden(_)
    ));
    // PRJ-047: a group in the team brings its members; PRJ-050: removing it takes them out.
    let carol = create_user(pool, &access, "carol").await;
    let group = groups::create(
        pool,
        &access,
        NewGroup {
            name: "Designers".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap();
    groups::add_members(pool, &access, group.id, &[carol])
        .await
        .unwrap();
    teams::add(pool, &access, id, &[], &[group.id])
        .await
        .unwrap();
    assert!(
        access_of(pool, carol)
            .await
            .has_in(Permission::IssueRead, id)
    );
    assert_eq!(projects::get(pool, &access, id).await.unwrap().team_size, 3);
    teams::remove(pool, &access, id, &[], &[group.id])
        .await
        .unwrap();
    assert!(
        !access_of(pool, carol)
            .await
            .has_in(Permission::IssueRead, id)
    );
    // Teams cannot join teams, and the generic group pages do not change teams.
    let other = create_project(pool, &access, Template::Blank, "Other", "OTH").await;
    let other_team = projects::get(pool, &access, other)
        .await
        .unwrap()
        .team
        .unwrap();
    assert!(
        teams::add(pool, &access, id, &[], &[other_team.id])
            .await
            .is_err()
    );
    assert!(matches!(
        groups::add_members(pool, &access, other_team.id, &[bob])
            .await
            .unwrap_err(),
        Error::Conflict(_)
    ));
}

#[tokio::test]
async fn acc_033_acc_050_prj_051_prj_052_team_roles_and_direct_roles() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let id = create_project(pool, &access, Template::Blank, "Roles", "ROL").await;
    let other = create_project(pool, &access, Template::Blank, "Elsewhere", "ELS").await;
    let team = projects::get(pool, &access, id)
        .await
        .unwrap()
        .team
        .unwrap();
    let viewer_role = roles::create(
        pool,
        &access,
        NewRole {
            name: "Deleter".to_owned(),
            permissions: vec!["project.delete".to_owned()],
            ..NewRole::default()
        },
    )
    .await
    .unwrap();
    // ACC-033: a team holds roles only in its own project.
    let error = roles::assign(
        pool,
        &access,
        viewer_role.role.id,
        Holder::Group(team.id),
        &[Some(other)],
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("scopes".to_owned(), Problem::Invalid)]);
    assert!(
        roles::assign(
            pool,
            &access,
            viewer_role.role.id,
            Holder::Group(team.id),
            &[None]
        )
        .await
        .is_err()
    );
    roles::assign(
        pool,
        &access,
        viewer_role.role.id,
        Holder::Group(team.id),
        &[Some(id)],
    )
    .await
    .unwrap();
    // ACC-050: a project administrator now cannot add people to the team: it would give them
    // project.delete, which the administrator lacks.
    let pat = create_user(pool, &access, "pat").await;
    roles::assign(
        pool,
        &access,
        role_id(pool, PredefinedRole::ProjectAdmin).await,
        Holder::User(pat),
        &[Some(id)],
    )
    .await
    .unwrap();
    let pat_access = access_of(pool, pat).await;
    let dan = create_user(pool, &access, "dan").await;
    match teams::add(pool, &pat_access, id, &[dan], &[])
        .await
        .unwrap_err()
    {
        Error::Forbidden(missing) => assert_eq!(missing.keys(), ["project.delete"]),
        other => panic!("{other:?}"),
    }
    // PRJ-052: direct roles in the project, by a project administrator, only there.
    roles::assign(
        pool,
        &pat_access,
        role_id(pool, PredefinedRole::ProjectMember).await,
        Holder::User(dan),
        &[Some(id)],
    )
    .await
    .unwrap();
    assert!(
        roles::assign(
            pool,
            &pat_access,
            role_id(pool, PredefinedRole::ProjectMember).await,
            Holder::User(dan),
            &[Some(other)],
        )
        .await
        .is_err()
    );
    let people = teams::people(pool, &pat_access, id).await.unwrap();
    assert_eq!(people.assignments.len(), 5);
    // Global roles that reach projects are listed read-only: the system administrator.
    assert_eq!(people.global.len(), 1);
    assert_eq!(
        people.global[0].role.builtin.as_deref(),
        Some("system_admin")
    );
}

#[tokio::test]
async fn prj_053_ownership_moves_with_the_administrator_role() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let id = create_project(pool, &access, Template::Blank, "Owned", "OWN").await;
    let erin = create_user(pool, &access, "erin").await;
    let row = projects::transfer(pool, &access, id, erin).await.unwrap();
    assert_eq!(row.project.owner_id, erin);
    let erin_access = access_of(pool, erin).await;
    assert!(erin_access.has_in(Permission::ProjectUpdate, id));
    // The previous owner keeps their roles.
    assert!(
        access_of(pool, admin.id)
            .await
            .has(Permission::ProjectUpdate)
    );
    // ACC-006: deleting the owner moves the project to the replacement.
    users::delete(pool, &access, &[erin], admin.id)
        .await
        .unwrap();
    let row = projects::get(pool, &access, id).await.unwrap();
    assert_eq!(row.project.owner_id, admin.id);
}

#[tokio::test]
async fn prj_061_prj_063_archived_projects_are_read_only_until_restored() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let id = create_project(pool, &access, Template::Blank, "Old", "OLD").await;
    projects::set_favorite(pool, &access, id, true)
        .await
        .unwrap();
    let row = projects::set_archived(pool, &access, id, true)
        .await
        .unwrap();
    assert!(row.project.is_archived());
    let error = projects::update(
        pool,
        &access,
        id,
        ProjectChange {
            name: Some("New".to_owned()),
            version: row.project.version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Conflict(_)));
    assert!(matches!(
        teams::add(pool, &access, id, &[admin.id], &[])
            .await
            .unwrap_err(),
        Error::Conflict(_)
    ));
    // PRJ-013: it lists only in the archived section.
    let active = projects::list(pool, &access, Section::Active, "", None, 50)
        .await
        .unwrap();
    assert!(active.is_empty());
    let archived = projects::list(pool, &access, Section::Archived, "", None, 50)
        .await
        .unwrap();
    assert_eq!(archived.len(), 1);
    let row = projects::set_archived(pool, &access, id, false)
        .await
        .unwrap();
    assert!(!row.project.is_archived());
    assert!(row.favorite);
}

#[tokio::test]
async fn prj_012_favorites_come_first() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    for (name, key) in [("Alpha", "AL"), ("Beta", "BE"), ("Gamma", "GA")] {
        create_project(pool, &access, Template::Blank, name, key).await;
    }
    let gamma = projects::by_key(pool, &access, "GA")
        .await
        .unwrap()
        .project
        .id;
    projects::set_favorite(pool, &access, gamma, true)
        .await
        .unwrap();
    let page = projects::list(pool, &access, Section::Active, "", None, 2)
        .await
        .unwrap();
    assert_eq!(
        page.iter()
            .map(|r| r.project.name.as_str())
            .collect::<Vec<_>>(),
        ["Gamma", "Alpha"]
    );
    let last = page.last().unwrap();
    let rest = projects::list(
        pool,
        &access,
        Section::Active,
        "",
        Some((1, last.project.name.to_lowercase(), last.project.id)),
        2,
    )
    .await
    .unwrap();
    assert_eq!(
        rest.iter()
            .map(|r| r.project.name.as_str())
            .collect::<Vec<_>>(),
        ["Beta"]
    );
    // The filter matches names and the start of keys.
    let found = projects::list(pool, &access, Section::Active, "be", None, 50)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn prj_064_prj_065_deletion_runs_in_the_background() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let id = create_project(pool, &access, Template::Scrum, "Doomed", "DOOM").await;
    let sets: Vec<i64> = project_fields::list(pool, &access, id)
        .await
        .unwrap()
        .iter()
        .filter(|row| row.shared_with == 0 && !row.field_default)
        .filter_map(|row| row.attached.set_id)
        .collect();
    assert!(!sets.is_empty());
    let member = create_user(pool, &access, "mia").await;
    teams::add(pool, &access, id, &[member], &[]).await.unwrap();
    // A member cannot delete it.
    assert!(matches!(
        projects::delete(pool, &access_of(pool, member).await, id)
            .await
            .unwrap_err(),
        Error::Forbidden(_)
    ));
    projects::delete(pool, &access, id).await.unwrap();
    // Nobody can use it now; it lists among deletions for those who may delete it.
    assert!(matches!(
        projects::get(pool, &access, id).await.unwrap_err(),
        Error::NotFound(_)
    ));
    let deleting = projects::list(pool, &access, Section::Deleting, "", None, 50)
        .await
        .unwrap();
    assert_eq!(deleting.len(), 1);
    assert!(deleting[0].deletion.is_some());
    let job: i64 = sqlx::query_scalar!("select id from jobs where kind = 'project.delete'")
        .fetch_one(pool)
        .await
        .unwrap();
    // Asking again while the job waits does not queue another.
    projects::delete(pool, &access, id).await.unwrap();
    let jobs: i64 =
        sqlx::query_scalar!(r#"select count(*) as "n!" from jobs where kind = 'project.delete'"#)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(jobs, 1);
    projects::run_deletion(pool, projects::ProjectDeletion { project: id })
        .await
        .unwrap();
    // Idempotent: a second run finds nothing to do.
    projects::run_deletion(pool, projects::ProjectDeletion { project: id })
        .await
        .unwrap();
    assert!(
        trackflow_store::projects::get(pool, id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(value_sets::get_many(pool, &sets).await.unwrap().is_empty());
    // The key is free again; the member lost the project's roles.
    create_project(pool, &access, Template::Blank, "Doomed", "DOOM").await;
    assert!(access_of(pool, member).await.projects().next().is_none());
    let _ = job;
}

#[tokio::test]
async fn acc_032_deleted_groups_hand_their_places_to_the_replacement() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let id = create_project(pool, &access, Template::Blank, "Places", "PLA").await;
    let old = groups::create(
        pool,
        &access,
        NewGroup {
            name: "Old".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap();
    let new = groups::create(
        pool,
        &access,
        NewGroup {
            name: "New".to_owned(),
            ..NewGroup::default()
        },
    )
    .await
    .unwrap();
    teams::add(pool, &access, id, &[], &[old.id]).await.unwrap();
    let row = projects::get(pool, &access, id).await.unwrap();
    projects::update(
        pool,
        &access,
        id,
        ProjectChange {
            audience: Some(Some(old.id)),
            version: row.project.version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap();
    groups::delete(pool, &access, &[old.id], new.id)
        .await
        .unwrap();
    let people = teams::people(pool, &access, id).await.unwrap();
    assert_eq!(
        people.team_groups.iter().map(|g| g.id).collect::<Vec<_>>(),
        [new.id]
    );
    let row = projects::get(pool, &access, id).await.unwrap();
    assert_eq!(row.audience.unwrap().id, new.id);
    // A team is deleted only with its project.
    assert!(
        groups::delete(pool, &access, &[row.team.unwrap().id], new.id)
            .await
            .is_err()
    );
    let _ = PermissionSet::EMPTY;
}

#[tokio::test]
async fn acc_022_prj_009_teams_of_hidden_projects_stay_hidden() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let mine = create_project(pool, &access, Template::Blank, "Mine", "MINE").await;
    let secret = create_project(pool, &access, Template::Blank, "Secret plans", "SEC").await;
    let pat = create_user(pool, &access, "pat").await;
    roles::assign(
        pool,
        &access,
        role_id(pool, PredefinedRole::ProjectAdmin).await,
        Holder::User(pat),
        &[Some(mine)],
    )
    .await
    .unwrap();
    let pat_access = access_of(pool, pat).await;
    let teams = trackflow_store::groups::Kinds {
        groups: false,
        teams: true,
    };
    let listed = groups::list(pool, &pat_access, "", teams, None, 50)
        .await
        .unwrap();
    let names: Vec<&str> = listed
        .iter()
        .map(|r| r.listed.group.name.as_str())
        .collect();
    assert_eq!(names, ["Mine"]);
    let hidden = projects::get(pool, &access, secret)
        .await
        .unwrap()
        .team
        .unwrap();
    assert!(matches!(
        groups::get(pool, &pat_access, hidden.id).await.unwrap_err(),
        Error::NotFound(_)
    ));
    // Whoever may see every project sees every team.
    let all = groups::list(pool, &access, "", teams, None, 50)
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn brd_003_lists_only_projects_of_the_viewers_teams() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool, "en").await.user;
    let access = access_of(pool, admin.id).await;
    let alpha = create_project(pool, &access, Template::Blank, "Alpha", "AL").await;
    let beta = create_project(pool, &access, Template::Blank, "Beta", "BE").await;
    // The creator joins the team; leaving Beta's, the administrator still sees it.
    teams::remove(pool, &access, beta, &[admin.id], &[])
        .await
        .unwrap();
    let access = access_of(pool, admin.id).await;
    let all = projects::list(pool, &access, Section::Active, "", None, 50)
        .await
        .unwrap();
    assert!(all.iter().any(|r| r.project.id == beta));
    let mine = projects::list_filtered(pool, &access, Section::Active, "", true, None, 50)
        .await
        .unwrap();
    assert_eq!(
        mine.iter().map(|r| r.project.id).collect::<Vec<_>>(),
        [alpha]
    );
}
