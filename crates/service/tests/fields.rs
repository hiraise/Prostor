//! Fields and value sets on a real database (step 1.2): global fields, value sets and their values,
//! fields in projects, and who may change what.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::access::Access;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::fields::{FieldKind, SetKind};
use taskmaster_domain::projects::Template;
use taskmaster_domain::settings::Language;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{FieldId, ProjectId, UserId};
use taskmaster_service::access;
use taskmaster_service::auth::SignedIn;
use taskmaster_service::fields::{self, FieldUpdate, NewFieldInput, SetSource};
use taskmaster_service::project_fields::{self, AttachInput, SetUse, SettingsChange};
use taskmaster_service::projects::{self, NewProjectInput};
use taskmaster_service::roles;
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::users::{self, NewAccount};
use taskmaster_service::value_sets::{self, ValueInput};
use taskmaster_service::{Error, Passwords};
use taskmaster_store::PgPool;
use taskmaster_store::access::Holder;
use taskmaster_store::sessions::Origin;
use taskmaster_testkit::TestDb;
use time::OffsetDateTime;
use time::macros::{date, datetime};

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);

fn passwords() -> Passwords {
    Passwords::new(4)
}

async fn set_up(pool: &PgPool) -> SignedIn {
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
            system_language: "en".to_owned(),
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

async fn project(pool: &PgPool, actor: &Access, name: &str, key: &str) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Blank,
            name: name.to_owned(),
            key: Some(key.to_owned()),
            first_number: None,
            description: String::new(),
        },
        Language::En,
    )
    .await
    .unwrap()
    .project
    .id
}

/// A project administrator of these projects.
async fn project_admin(pool: &PgPool, actor: &Access, username: &str, of: &[ProjectId]) -> Access {
    let user = create_user(pool, actor, username).await;
    let role = taskmaster_store::roles::builtin_id(pool, PredefinedRole::ProjectAdmin.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().copied().map(Some).collect();
    roles::assign(pool, actor, role, Holder::User(user), &scopes)
        .await
        .unwrap();
    access_of(pool, user).await
}

fn problems(error: Error) -> Vec<(String, Problem)> {
    match error {
        Error::Invalid(problems) => problems.into_iter().map(|p| (p.path, p.problem)).collect(),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

fn new_field(name: &str, kind: FieldKind) -> NewFieldInput {
    NewFieldInput {
        name: name.to_owned(),
        kind,
        multi: false,
        aliases: Vec::new(),
        set: SetSource::New { name: None },
        can_be_empty: true,
        empty_label: String::new(),
        private: false,
        default_values: Vec::new(),
        auto_attach: false,
    }
}

async fn field_id(pool: &PgPool, access: &Access, name: &str) -> FieldId {
    fields::list(pool, access, name, None, None, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.field.name == name)
        .unwrap()
        .field
        .id
}

#[tokio::test]
async fn fld_015_016_017_022_global_fields() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    // FLD-016: one field per name and type; the same name with another type is another field.
    let error = fields::create(pool, &access, new_field("priority", FieldKind::Enum))
        .await
        .unwrap_err();
    assert_eq!(problems(error), [("name".to_owned(), Problem::Taken)]);
    let multi = fields::create(
        pool,
        &access,
        NewFieldInput {
            multi: true,
            aliases: vec!["reviewers".to_owned(), "Reviewers".to_owned()],
            ..new_field("Reviewer", FieldKind::User)
        },
    )
    .await
    .unwrap();
    // FLD-017: aliases are distinct ignoring case.
    assert_eq!(multi.field.aliases, ["reviewers"]);
    assert_eq!(multi.set.as_ref().unwrap().name, "Reviewer");
    let error = fields::create(pool, &access, new_field("a/b", FieldKind::String))
        .await
        .unwrap_err();
    assert_eq!(problems(error), [("name".to_owned(), Problem::Invalid)]);
    let error = fields::create(
        pool,
        &access,
        NewFieldInput {
            multi: true,
            ..new_field("Stage", FieldKind::State)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("multi".to_owned(), Problem::Invalid)]);
    // FLD-015: a field may reuse another field's set and then choose defaults from it.
    let priority = fields::get(pool, &access, field_id(pool, &access, "Priority").await)
        .await
        .unwrap();
    let urgency = fields::create(
        pool,
        &access,
        NewFieldInput {
            set: SetSource::Existing(priority.field.set_id.unwrap()),
            default_values: vec![priority.default_values[0].id],
            ..new_field("Urgency", FieldKind::Enum)
        },
    )
    .await
    .unwrap();
    assert_eq!(urgency.set.unwrap().id, priority.field.set_id.unwrap());
    assert_eq!(urgency.default_values[0].name, "Normal");
    // FLD-022: the instance order changes by moving.
    fields::move_after(pool, &access, urgency.field.id, None)
        .await
        .unwrap();
    let all = fields::list(pool, &access, "", None, None, 100)
        .await
        .unwrap();
    assert_eq!(all[0].field.name, "Urgency");
    fields::move_after(pool, &access, urgency.field.id, Some(priority.field.id))
        .await
        .unwrap();
    let all = fields::list(pool, &access, "", None, None, 100)
        .await
        .unwrap();
    assert_eq!(all[1].field.name, "Urgency");
    // FLD-025: unused fields can be listed on their own.
    let unused = fields::list(pool, &access, "", Some(true), None, 100)
        .await
        .unwrap();
    assert!(unused.iter().any(|row| row.field.name == "Due date"));
    assert!(unused.iter().all(|row| row.project_count == 0));
}

#[tokio::test]
async fn fld_018_019_027_who_changes_base_properties_and_defaults() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let one = project(pool, &access, "One", "ONE").await;
    let two = project(pool, &access, "Two", "TWO").await;
    let pat = project_admin(pool, &access, "pat", &[one]).await;
    // FLD-052: a project administrator creates a field from the project.
    let row = project_fields::attach(
        pool,
        &pat,
        one,
        AttachInput::New {
            name: "Customer".to_owned(),
            kind: FieldKind::Enum,
            multi: false,
            aliases: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert_eq!(row.set.as_ref().unwrap().name, "One: Customer");
    let field = row.field.id;
    // FLD-018: only used in projects they configure, so they rename it.
    let current = fields::get(pool, &pat, field).await.unwrap();
    let renamed = fields::update(
        pool,
        &pat,
        field,
        FieldUpdate {
            name: Some("Client".to_owned()),
            version: current.field.version,
            ..FieldUpdate::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.field.name, "Client");
    // Once another project uses it, they no longer may.
    project_fields::attach(
        pool,
        &access,
        two,
        AttachInput::Existing {
            field,
            set: SetUse::Shared,
        },
    )
    .await
    .unwrap();
    let error = fields::update(
        pool,
        &pat,
        field,
        FieldUpdate {
            name: Some("Account".to_owned()),
            version: renamed.field.version,
            ..FieldUpdate::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Forbidden(_)));
    // Auto-attached fields and the settings of new attachments belong to the instance.
    let priority = field_id(pool, &access, "Priority").await;
    let current = fields::get(pool, &pat, priority).await.unwrap();
    assert!(matches!(
        fields::update(
            pool,
            &pat,
            priority,
            FieldUpdate {
                aliases: Some(vec!["prio".to_owned()]),
                version: current.field.version,
                ..FieldUpdate::default()
            },
        )
        .await
        .unwrap_err(),
        Error::Forbidden(_)
    ));
    // FLD-056: a field that cannot be empty has no empty label.
    let error = fields::update(
        pool,
        &access,
        priority,
        FieldUpdate {
            empty_label: Some("None".to_owned()),
            version: current.field.version,
            ..FieldUpdate::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(error),
        [("empty_label".to_owned(), Problem::Invalid)]
    );
    // FLD-019: global defaults apply to new attachments only.
    let updated = fields::update(
        pool,
        &access,
        priority,
        FieldUpdate {
            private: Some(true),
            version: current.field.version,
            ..FieldUpdate::default()
        },
    )
    .await
    .unwrap();
    assert!(updated.field.private);
    let in_one = project_fields::list(pool, &access, one).await.unwrap();
    assert!(!in_one[0].attached.private);
    let three = project(pool, &access, "Three", "THR").await;
    let in_three = project_fields::list(pool, &access, three).await.unwrap();
    assert!(in_three[0].attached.private);
    // Members who configure nothing do not see the field list.
    let mia = create_user(pool, &access, "mia").await;
    assert!(
        fields::list(pool, &access_of(pool, mia).await, "", None, None, 10)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn fld_030_031_032_037_044_045_shared_sets_and_their_values() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let one = project(pool, &access, "One", "ONE").await;
    let two = project(pool, &access, "Two", "TWO").await;
    let pat = project_admin(pool, &access, "pat", &[one]).await;
    let fields_one = project_fields::list(pool, &access, one).await.unwrap();
    let types = &fields_one[1];
    let shared = types.attached.set_id.unwrap();
    // FLD-031: the Types set is shared by both projects; Pat configures only one of them.
    // They see the project they configure and learn that one more uses it.
    let set = value_sets::get(pool, &pat, shared).await.unwrap();
    assert_eq!((set.projects.len(), set.hidden_projects), (1, 1));
    assert!(!set.editable);
    let error = value_sets::add_value(
        pool,
        &pat,
        shared,
        ValueInput {
            name: Some("Chore".to_owned()),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Forbidden(_)));
    // FLD-030: so they fork it; defaults follow their copies.
    let forked = project_fields::fork_set(pool, &pat, one, types.field.id)
        .await
        .unwrap();
    let own = forked.attached.set_id.unwrap();
    assert_ne!(own, shared);
    assert_eq!(forked.set.as_ref().unwrap().name, "One: Types");
    assert_eq!(forked.default_values[0].name, "Task");
    assert_ne!(forked.default_values[0].id, types.default_values[0].id);
    assert_eq!(forked.shared_with, 0);
    // FLD-032: values of the own set change freely.
    let set = value_sets::add_value(
        pool,
        &pat,
        own,
        ValueInput {
            name: Some("Chore".to_owned()),
            color: Some(Some(4)),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap();
    let chore = set.values.last().unwrap().clone();
    assert_eq!(chore.name, "Chore");
    assert_eq!(chore.color, Some(4));
    // FLD-028, FLD-044: names are unique ignoring case; colors come from the palette.
    let error = value_sets::add_value(
        pool,
        &pat,
        own,
        ValueInput {
            name: Some("chore".to_owned()),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("name".to_owned(), Problem::Taken)]);
    assert!(
        value_sets::add_value(
            pool,
            &pat,
            own,
            ValueInput {
                name: Some("Odd".to_owned()),
                color: Some(Some(99)),
                ..ValueInput::default()
            },
            None,
        )
        .await
        .is_err()
    );
    // FLD-037: values move.
    let set = value_sets::move_value(pool, &pat, own, chore.id, None)
        .await
        .unwrap();
    assert_eq!(set.values[0].name, "Chore");
    let bug = set.values.iter().find(|v| v.name == "Bug").unwrap().id;
    let set = value_sets::move_value(pool, &pat, own, chore.id, Some(bug))
        .await
        .unwrap();
    let names: Vec<&str> = set.values.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(&names[..3], ["Bug", "Chore", "Feature"]);
    // FLD-045: archiving the default value takes it out of the defaults.
    let task = set
        .values
        .iter()
        .find(|v| v.name == "Task")
        .unwrap()
        .clone();
    value_sets::update_value(
        pool,
        &pat,
        own,
        task.id,
        ValueInput {
            archived: Some(true),
            ..ValueInput::default()
        },
        task.version,
    )
    .await
    .unwrap();
    let after = project_fields::list(pool, &pat, one).await.unwrap();
    assert!(after[1].default_values.is_empty());
    // Other kinds' properties are refused.
    let error = value_sets::update_value(
        pool,
        &pat,
        own,
        chore.id,
        ValueInput {
            resolved: Some(true),
            ..ValueInput::default()
        },
        set.values
            .iter()
            .find(|v| v.id == chore.id)
            .unwrap()
            .version,
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("resolved".to_owned(), Problem::Invalid)]);
    // The other project still has the shared set with its default.
    let two_fields = project_fields::list(pool, &access, two).await.unwrap();
    assert_eq!(two_fields[1].attached.set_id, Some(shared));
    assert_eq!(two_fields[1].default_values[0].name, "Task");
}

#[tokio::test]
async fn fld_046_047_048_049_value_properties_by_kind() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let id = project(pool, &access, "Kinds", "KIN").await;
    let list = project_fields::list(pool, &access, id).await.unwrap();
    let component = list.iter().find(|r| r.field.name == "Component").unwrap();
    let versions = list
        .iter()
        .find(|r| r.field.name == "Fix versions")
        .unwrap();
    let builds = list
        .iter()
        .find(|r| r.field.name == "Fixed in build")
        .unwrap();
    let owner = create_user(pool, &access, "olga").await;
    let set = value_sets::add_value(
        pool,
        &access,
        component.attached.set_id.unwrap(),
        ValueInput {
            name: Some("Backend".to_owned()),
            owner: Some(Some(owner)),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(set.values[0].owner_id, Some(owner));
    assert_eq!(set.owners[0].id, owner);
    let set = value_sets::add_value(
        pool,
        &access,
        versions.attached.set_id.unwrap(),
        ValueInput {
            name: Some("2.0".to_owned()),
            released: Some(true),
            release_date: Some(Some(date!(2026 - 11 - 01))),
            start_date: Some(Some(date!(2026 - 10 - 01))),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap();
    assert!(set.values[0].released);
    // The affected-versions field shows the same list.
    let affected = list
        .iter()
        .find(|r| r.field.name == "Affected versions")
        .unwrap();
    assert_eq!(affected.attached.set_id, versions.attached.set_id);
    let set = value_sets::add_value(
        pool,
        &access,
        builds.attached.set_id.unwrap(),
        ValueInput {
            name: Some("1042".to_owned()),
            assembled_on: Some(Some(date!(2026 - 10 - 20))),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(set.values[0].assembled_on, Some(date!(2026 - 10 - 20)));
    // A state set takes the resolved flag; a version set does not.
    let error = value_sets::add_value(
        pool,
        &access,
        versions.attached.set_id.unwrap(),
        ValueInput {
            name: Some("3.0".to_owned()),
            resolved: Some(true),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("resolved".to_owned(), Problem::Invalid)]);
    // FLD-040: user sets list people and groups, not values.
    let assignee = list.iter().find(|r| r.field.name == "Assignee").unwrap();
    let assignees = assignee.attached.set_id.unwrap();
    assert!(
        value_sets::add_value(
            pool,
            &access,
            assignees,
            ValueInput {
                name: Some("x".to_owned()),
                ..ValueInput::default()
            },
            None
        )
        .await
        .is_err()
    );
    let set = value_sets::add_members(pool, &access, assignees, &[owner], &[])
        .await
        .unwrap();
    assert_eq!(set.users.len(), 1);
    assert_eq!(set.groups.len(), 1, "the team");
    // A default assignee must be offered by the set.
    let stranger = create_user(pool, &access, "sam").await;
    let error = project_fields::update(
        pool,
        &access,
        id,
        assignee.field.id,
        SettingsChange {
            default_values: Some(vec![stranger]),
            version: assignee.attached.version,
            ..SettingsChange::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(error),
        [("default_values".to_owned(), Problem::Invalid)]
    );
    let row = project_fields::update(
        pool,
        &access,
        id,
        assignee.field.id,
        SettingsChange {
            default_values: Some(vec![admin.id]),
            version: assignee.attached.version,
            ..SettingsChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(row.default_users[0].id, admin.id);
}

#[tokio::test]
async fn fld_051_to_058_fields_in_a_project() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = set_up(pool).await.user;
    let access = access_of(pool, admin.id).await;
    let id = project(pool, &access, "Config", "CFG").await;
    // FLD-053: attach an existing field with a copy of its set.
    let due = field_id(pool, &access, "Due date").await;
    let row = project_fields::attach(
        pool,
        &access,
        id,
        AttachInput::Existing {
            field: due,
            set: SetUse::Copy,
        },
    )
    .await
    .unwrap();
    assert_eq!(row.field.name, "Due date");
    assert!(row.set.is_none());
    assert_eq!(row.attached.empty_label, "No due date");
    // Twice is refused; so is another field with the same name.
    assert!(matches!(
        project_fields::attach(
            pool,
            &access,
            id,
            AttachInput::Existing {
                field: due,
                set: SetUse::Shared
            }
        )
        .await
        .unwrap_err(),
        Error::Conflict(_)
    ));
    let text_due = fields::create(pool, &access, new_field("Due date", FieldKind::Text))
        .await
        .unwrap();
    let error = project_fields::attach(
        pool,
        &access,
        id,
        AttachInput::Existing {
            field: text_due.field.id,
            set: SetUse::Shared,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(problems(error), [("field".to_owned(), Problem::Taken)]);
    // FLD-056, FLD-057: a field that cannot be empty and has no default is required.
    let row = project_fields::update(
        pool,
        &access,
        id,
        due,
        SettingsChange {
            can_be_empty: Some(false),
            version: row.attached.version,
            ..SettingsChange::default()
        },
    )
    .await
    .unwrap();
    assert!(row.required);
    assert_eq!(row.attached.empty_label, "");
    // FLD-058: the project's order.
    let list = project_fields::move_after(pool, &access, id, due, None)
        .await
        .unwrap();
    assert_eq!(list[0].field.name, "Due date");
    let priority = list
        .iter()
        .find(|r| r.field.name == "Priority")
        .unwrap()
        .field
        .id;
    let list = project_fields::move_after(pool, &access, id, due, Some(priority))
        .await
        .unwrap();
    assert_eq!(list[1].field.name, "Due date");
    // FLD-054: detaching deletes a set only this project used.
    let component = list.iter().find(|r| r.field.name == "Component").unwrap();
    let set = component.attached.set_id.unwrap();
    project_fields::detach(pool, &access, id, component.field.id)
        .await
        .unwrap();
    assert!(
        taskmaster_store::value_sets::get(pool, set)
            .await
            .unwrap()
            .is_none()
    );
    // FLD-023: deleting a field removes it from every project.
    assert_eq!(
        fields::deletion_impact(pool, &access, due).await.unwrap(),
        (1, 0, 0)
    );
    fields::delete(pool, &access, due).await.unwrap();
    let list = project_fields::list(pool, &access, id).await.unwrap();
    assert!(list.iter().all(|r| r.field.id != due));
    // FLD-029: sets are listed with their use.
    let sets = value_sets::list(pool, &access, Some(SetKind::State), "", None, 50)
        .await
        .unwrap();
    assert!(
        sets.iter()
            .any(|s| s.set.name == "States" && s.projects == 1)
    );
}
