//! Issues on a real database (step 1.3): drafts and numbers, readable and former IDs, field
//! values with defaults, required fields and resolution, texts with conflicts, audiences and the
//! list predicate, private fields, moves, tags, stars and votes, history, and what changes of
//! fields, accounts, groups and projects do to issues.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prostor_domain::access::Access;
use prostor_domain::access::permission::Permission;
use prostor_domain::access::role::PredefinedRole;
use prostor_domain::fields::FieldKind;
use prostor_domain::issues::history::Origin;
use prostor_domain::issues::values::{Input, Value};
use prostor_domain::projects::Template;
use prostor_domain::settings::Language;
use prostor_domain::validation::Problem;
use prostor_domain::{FieldId, IssueId, ProjectId, UserId};
use prostor_service::auth::SignedIn;
use prostor_service::issues::jobs::{self, FieldWork};
use prostor_service::issues::{
    self, AudienceInput, FieldChange, FieldInput, IssueChange, IssueView, NewIssue,
};
use prostor_service::project_fields;
use prostor_service::projects::{self, NewProjectInput, ProjectChange};
use prostor_service::search::{self, SearchInput};
use prostor_service::setup::{self, Setup};
use prostor_service::users::{self, NewAccount};
use prostor_service::{Error, Passwords, access, fields, groups, roles, tags, value_sets};
use prostor_store::PgPool;
use prostor_store::access::Holder;
use prostor_store::issue_history::Direction;
use prostor_store::sessions::Origin as SessionOrigin;
use prostor_store::tags::Listing;
use prostor_store::value_sets::Value as SetValue;
use prostor_testkit::TestDb;
use serde_json::json;
use time::OffsetDateTime;
use time::macros::datetime;

/// A project's issues as the list shows them, newest first (the search with a project context).
async fn list(pool: &PgPool, viewer: &Access, project: ProjectId, limit: u32) -> issues::Described {
    search::search(
        pool,
        viewer,
        SearchInput {
            text: "sort: id".to_owned(),
            project: Some(project),
            limit,
            ..SearchInput::default()
        },
    )
    .await
    .unwrap()
    .issues
}

/// How many of a project's issues the viewer sees: all and unresolved.
async fn count(pool: &PgPool, viewer: &Access, project: ProjectId) -> (i64, i64) {
    search::count(
        pool,
        viewer,
        SearchInput {
            project: Some(project),
            limit: 1,
            ..SearchInput::default()
        },
    )
    .await
    .unwrap()
}

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const WEB: Origin = Origin::Web;

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
        &SessionOrigin::default(),
    )
    .await
    .unwrap()
}

async fn admin(pool: &PgPool) -> Access {
    let signed_in = set_up(pool).await;
    access::load(pool, signed_in.user.id).await.unwrap()
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

/// A person holding a predefined role in these projects.
async fn person(
    pool: &PgPool,
    actor: &Access,
    username: &str,
    role: PredefinedRole,
    of: &[ProjectId],
) -> Access {
    let user = create_user(pool, actor, username).await;
    grant(pool, actor, user, role, of).await;
    access::load(pool, user).await.unwrap()
}

async fn grant(
    pool: &PgPool,
    actor: &Access,
    user: UserId,
    role: PredefinedRole,
    of: &[ProjectId],
) {
    let role = prostor_store::roles::builtin_id(pool, role.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().copied().map(Some).collect();
    roles::assign(pool, actor, role, Holder::User(user), &scopes)
        .await
        .unwrap();
}

fn new_issue(project: ProjectId, summary: &str) -> NewIssue {
    NewIssue {
        project,
        summary: summary.to_owned(),
        publish: true,
        ..NewIssue::default()
    }
}

async fn create(pool: &PgPool, actor: &Access, project: ProjectId, summary: &str) -> IssueView {
    issues::create(pool, actor, WEB, new_issue(project, summary))
        .await
        .unwrap()
        .done
        .0
}

fn field<'a>(view: &'a IssueView, name: &str) -> &'a issues::FieldView {
    view.fields
        .iter()
        .find(|f| f.field.field.name == name)
        .unwrap_or_else(|| panic!("no field {name}"))
}

fn field_id(view: &IssueView, name: &str) -> FieldId {
    field(view, name).field.id()
}

/// Values of the set a field uses in the issue's project.
async fn values_of(pool: &PgPool, view: &IssueView, name: &str) -> Vec<SetValue> {
    let set = field(view, name).field.attached.set_id.unwrap();
    prostor_store::value_sets::values_of(pool, &[set])
        .await
        .unwrap()
}

fn problems(error: Error) -> Vec<(String, Problem)> {
    match error {
        Error::Invalid(problems) => problems.into_iter().map(|p| (p.path, p.problem)).collect(),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

async fn kinds(pool: &PgPool, issue: IssueId) -> Vec<String> {
    prostor_store::issue_history::all_of(pool, issue)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect()
}

/// Change records of a page of the activity stream, with the cursor of the next page.
struct History {
    entries: Vec<prostor_store::issue_history::Entry>,
    next: Option<i64>,
}

async fn history(
    pool: &PgPool,
    viewer: &Access,
    issue: IssueId,
    from: Option<i64>,
    direction: Direction,
    limit: i64,
) -> Result<History, Error> {
    let page = issues::activity::page(
        pool,
        viewer,
        issue,
        issues::activity::Query {
            comments: false,
            changes: true,
            from,
            direction,
            limit,
            around_comment: None,
        },
    )
    .await?;
    Ok(History {
        next: match direction {
            Direction::Older => page.older,
            Direction::Newer => page.newer,
        },
        entries: page.items.iter().map(|i| i.entry().clone()).collect(),
    })
}

async fn set(
    pool: &PgPool,
    actor: &Access,
    issue: IssueId,
    field: FieldId,
    change: FieldChange,
) -> Result<IssueView, Error> {
    issues::set_field(pool, actor, WEB, issue, field, change, false)
        .await
        .map(|answer| answer.done.0)
}

#[tokio::test]
async fn iss_001_iss_002_prj_004_fld_055_issues_get_numbers_and_defaults() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;

    let first = create(pool, &admin, web, "  First\nissue ").await;
    assert_eq!(first.issue.number, Some(1));
    assert_eq!(first.readable_id().as_deref(), Some("WEB-1"));
    assert_eq!(first.issue.summary, "First issue");
    assert_eq!(first.issue.reporter_id, admin.user());
    // Defaults of the project's settings fill the fields (FLD-055).
    let priority = field(&first, "Priority");
    let normal = values_of(pool, &first, "Priority")
        .await
        .into_iter()
        .find(|v| v.name == "Normal")
        .unwrap();
    assert_eq!(priority.value, Some(Value::Refs(vec![normal.id])));
    assert!(field(&first, "Assignee").value.is_none());
    assert_eq!(kinds(pool, first.issue.id).await, ["created"]);

    // A draft takes no number; publishing it takes the next one (ISS-002).
    let draft = issues::create(
        pool,
        &admin,
        WEB,
        NewIssue {
            publish: false,
            ..new_issue(web, "")
        },
    )
    .await
    .unwrap()
    .done
    .0;
    assert_eq!(draft.issue.number, None);
    let second = create(pool, &admin, web, "Second").await;
    assert_eq!(second.issue.number, Some(2));
    assert!(kinds(pool, draft.issue.id).await.is_empty());
    let err = issues::publish(pool, &admin, WEB, draft.issue.id)
        .await
        .unwrap_err();
    assert_eq!(problems(err), [("summary".to_owned(), Problem::Required)]);
    issues::update(
        pool,
        &admin,
        WEB,
        draft.issue.id,
        IssueChange {
            summary: Some("Third".to_owned()),
            text_version: Some(draft.issue.text_version),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    let third = issues::publish(pool, &admin, WEB, draft.issue.id)
        .await
        .unwrap()
        .done
        .0;
    assert_eq!(third.issue.number, Some(3));
    assert_eq!(kinds(pool, third.issue.id).await, ["created"]);
    let project = projects::get(pool, &admin, web).await.unwrap();
    assert_eq!(project.project.next_number, 4);
}

#[tokio::test]
async fn iss_002_drafts_are_their_reporters_alone() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let draft = issues::create(
        pool,
        &member,
        WEB,
        NewIssue {
            publish: false,
            ..new_issue(web, "Idea")
        },
    )
    .await
    .unwrap()
    .done
    .0;
    assert!(matches!(
        issues::get(pool, &admin, draft.issue.id).await,
        Err(Error::NotFound(_))
    ));
    assert_eq!(issues::drafts(pool, &member).await.unwrap().issues.len(), 1);
    assert!(list(pool, &member, web, 50).await.issues.is_empty());
    issues::delete(pool, &member, WEB, draft.issue.id)
        .await
        .unwrap();
    assert!(
        issues::drafts(pool, &member)
            .await
            .unwrap()
            .issues
            .is_empty()
    );
}

#[tokio::test]
async fn iss_003_prj_005_former_ids_lead_to_the_issue() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let app = project(pool, &admin, "App", "APP").await;
    let issue = create(pool, &admin, web, "Moving").await;
    let id = issue.issue.id;
    assert_eq!(
        issues::resolve(pool, &admin, "web-1").await.unwrap(),
        (id, true)
    );
    // The key changes: the old one still leads to it.
    let version = projects::get(pool, &admin, web)
        .await
        .unwrap()
        .project
        .version;
    projects::update(
        pool,
        &admin,
        web,
        ProjectChange {
            key: Some("SITE".to_owned()),
            version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        issues::resolve(pool, &admin, "WEB-1").await.unwrap(),
        (id, false)
    );
    assert_eq!(
        issues::resolve(pool, &admin, "SITE-1").await.unwrap(),
        (id, true)
    );
    // It moves: the target's next number, the old ones stay aliases.
    let moved = issues::moves::move_issue(pool, &admin, WEB, id, app, false)
        .await
        .unwrap()
        .done
        .unwrap()
        .0;
    assert_eq!(moved.readable_id().as_deref(), Some("APP-1"));
    assert_eq!(
        issues::resolve(pool, &admin, "SITE-1").await.unwrap(),
        (id, false)
    );
    assert_eq!(
        issues::resolve(pool, &admin, "WEB-1").await.unwrap(),
        (id, false)
    );
    assert_eq!(
        issues::resolve(pool, &admin, "APP-1").await.unwrap(),
        (id, true)
    );
    // Numbers are not reused.
    let next = create(pool, &admin, web, "Next").await;
    assert_eq!(next.readable_id().as_deref(), Some("SITE-2"));
    assert!(matches!(
        issues::resolve(pool, &admin, "APP-9").await,
        Err(Error::NotFound(_))
    ));
    let records = prostor_store::issue_history::all_of(pool, id)
        .await
        .unwrap();
    let last = records.last().unwrap();
    assert_eq!(last.kind, "project");
    assert_eq!(last.removed.as_ref().unwrap()["id"], json!("SITE-1"));
    assert_eq!(last.added.as_ref().unwrap()["id"], json!("APP-1"));
}

#[tokio::test]
async fn fld_001_fld_057_iss_019_iss_023_field_changes_and_resolution() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Bug").await;
    let id = issue.issue.id;
    assert!(issue.issue.resolved_at.is_none());

    // A state flagged resolved resolves the issue; an unresolved one reopens it (ISS-023).
    let state = field_id(&issue, "State");
    let states = values_of(pool, &issue, "State").await;
    let fixed = states.iter().find(|v| v.resolved).unwrap();
    let open = states.iter().find(|v| !v.resolved).unwrap();
    let view = set(
        pool,
        &admin,
        id,
        state,
        FieldChange::Set(Some(Input::Integer(fixed.id))),
    )
    .await
    .unwrap();
    assert!(view.issue.resolved_at.is_some());
    let view = set(
        pool,
        &admin,
        id,
        state,
        FieldChange::Set(Some(Input::Integer(open.id))),
    )
    .await
    .unwrap();
    assert!(view.issue.resolved_at.is_none());
    let k = kinds(pool, id).await;
    assert_eq!(k, ["created", "field", "resolved", "field", "reopened"]);

    // A field that cannot be empty cannot be cleared (FLD-057).
    let err = set(pool, &admin, id, state, FieldChange::Set(None))
        .await
        .unwrap_err();
    assert_eq!(problems(err), [("value".to_owned(), Problem::Required)]);
    // A value of another set is refused.
    let priority = values_of(pool, &issue, "Priority").await;
    let err = set(
        pool,
        &admin,
        id,
        state,
        FieldChange::Set(Some(Input::Integer(priority[0].id))),
    )
    .await
    .unwrap_err();
    assert_eq!(problems(err), [("value".to_owned(), Problem::Invalid)]);

    // Simple values and their limits (FLD-004, FLD-007, FLD-028).
    let estimation = field_id(&issue, "Estimation");
    let view = set(
        pool,
        &admin,
        id,
        estimation,
        FieldChange::Set(Some(Input::Text("1h 30m".to_owned()))),
    )
    .await
    .unwrap();
    assert_eq!(field(&view, "Estimation").value, Some(Value::Duration(90)));
    // Spent time is computed (ISS-019).
    let spent = field_id(&issue, "Spent time");
    assert!(matches!(
        set(
            pool,
            &admin,
            id,
            spent,
            FieldChange::Set(Some(Input::Integer(5)))
        )
        .await,
        Err(Error::Conflict(_))
    ));

    // Multi-valued fields: add and remove without touching the others (FLD-001).
    let versions = field_id(&issue, "Fix versions");
    let set_id = field(&issue, "Fix versions").field.attached.set_id.unwrap();
    let mut ids = Vec::new();
    for name in ["1.0", "1.1"] {
        let row = value_sets::add_value(
            pool,
            &admin,
            set_id,
            value_sets::ValueInput {
                name: Some(name.to_owned()),
                ..value_sets::ValueInput::default()
            },
            None,
        )
        .await
        .unwrap();
        ids.push(row.values.iter().find(|v| v.name == name).unwrap().id);
    }
    set(pool, &admin, id, versions, FieldChange::Add(vec![ids[0]]))
        .await
        .unwrap();
    let view = set(
        pool,
        &admin,
        id,
        versions,
        FieldChange::Add(vec![ids[1], ids[0]]),
    )
    .await
    .unwrap();
    let Some(Value::Refs(mut held)) = field(&view, "Fix versions").value.clone() else {
        panic!("no versions")
    };
    held.sort_unstable();
    assert_eq!(held, ids);
    let view = set(
        pool,
        &admin,
        id,
        versions,
        FieldChange::Remove(vec![ids[0]]),
    )
    .await
    .unwrap();
    assert_eq!(
        field(&view, "Fix versions").value,
        Some(Value::Refs(vec![ids[1]]))
    );
    assert!(matches!(
        set(pool, &admin, id, state, FieldChange::Add(vec![open.id])).await,
        Err(Error::Invalid(_))
    ));
    // Each value separately in history (HIST-004).
    let last = prostor_store::issue_history::all_of(pool, id)
        .await
        .unwrap();
    let last = last.last().unwrap();
    assert_eq!(last.removed, Some(json!([{ "id": ids[0], "name": "1.0" }])));
    assert_eq!(last.added, Some(json!([])));
}

#[tokio::test]
async fn fld_057_required_fields_block_publishing_only() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Probe").await;
    let assignee = field_id(&issue, "Assignee");
    project_fields::update(
        pool,
        &admin,
        web,
        assignee,
        project_fields::SettingsChange {
            can_be_empty: Some(false),
            empty_label: Some(String::new()),
            version: field(&issue, "Assignee").field.attached.version,
            ..project_fields::SettingsChange::default()
        },
    )
    .await
    .unwrap();
    let err = issues::create(pool, &admin, WEB, new_issue(web, "Needs one"))
        .await
        .unwrap_err();
    assert_eq!(
        problems(err),
        [(format!("fields.{assignee}"), Problem::Required)]
    );
    let draft = issues::create(
        pool,
        &admin,
        WEB,
        NewIssue {
            publish: false,
            ..new_issue(web, "Needs one")
        },
    )
    .await
    .unwrap()
    .done
    .0;
    set(
        pool,
        &admin,
        draft.issue.id,
        assignee,
        FieldChange::Set(Some(Input::Integer(admin.user()))),
    )
    .await
    .unwrap();
    let issue = issues::publish(pool, &admin, WEB, draft.issue.id)
        .await
        .unwrap()
        .done
        .0;
    assert_eq!(
        field(&issue, "Assignee").value,
        Some(Value::Refs(vec![admin.user()]))
    );
    // An account outside the user set is refused (FLD-010).
    let stranger = create_user(pool, &admin, "stranger").await;
    let err = set(
        pool,
        &admin,
        issue.issue.id,
        assignee,
        FieldChange::Set(Some(Input::Integer(stranger))),
    )
    .await
    .unwrap_err();
    assert_eq!(problems(err), [("value".to_owned(), Problem::Invalid)]);
}

#[tokio::test]
async fn iss_051_text_edits_carry_their_version() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Text").await;
    let id = issue.issue.id;
    let started = issue.issue.text_version;
    let edit = |summary: &str, version: i64| IssueChange {
        summary: Some(summary.to_owned()),
        description: Some("Body".to_owned()),
        text_version: Some(version),
        ..IssueChange::default()
    };
    let view = issues::update(pool, &admin, WEB, id, edit("Text one", started))
        .await
        .unwrap()
        .done
        .0;
    assert_eq!(view.issue.description, "Body");
    // A field change in between does not conflict with a text edit.
    let state = field_id(&issue, "Priority");
    let high = values_of(pool, &issue, "Priority").await[0].id;
    set(
        pool,
        &admin,
        id,
        state,
        FieldChange::Set(Some(Input::Integer(high))),
    )
    .await
    .unwrap();
    // Another edit from the old version does.
    assert!(matches!(
        issues::update(pool, &admin, WEB, id, edit("Text two", started)).await,
        Err(Error::Conflict(_))
    ));
    issues::update(
        pool,
        &admin,
        WEB,
        id,
        edit("Text two", view.issue.text_version),
    )
    .await
    .unwrap();
    let records = prostor_store::issue_history::all_of(pool, id)
        .await
        .unwrap();
    let texts: Vec<_> = records
        .iter()
        .filter(|e| e.kind == "summary")
        .map(|e| (e.removed.clone().unwrap(), e.added.clone().unwrap()))
        .collect();
    assert_eq!(
        texts,
        [
            (json!("Text"), json!("Text one")),
            (json!("Text one"), json!("Text two"))
        ]
    );
    // Without a version the edit is refused.
    let err = issues::update(
        pool,
        &admin,
        WEB,
        id,
        IssueChange {
            summary: Some("x".to_owned()),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        problems(err),
        [("text_version".to_owned(), Problem::Required)]
    );
}

#[tokio::test]
async fn iss_041_iss_044_acc_048_acc_049_audiences_and_the_list_predicate() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let other = person(pool, &admin, "otto", PredefinedRole::ProjectMember, &[web]).await;
    // A submit-only role: create without read (ACC-048).
    let submitter = create_user(pool, &admin, "sam").await;
    let role = roles::create(
        pool,
        &admin,
        roles::NewRole {
            name: "Submitter".to_owned(),
            description: String::new(),
            permissions: ["project.basic", "issue.create"]
                .map(str::to_owned)
                .to_vec(),
        },
    )
    .await
    .unwrap();
    roles::assign(
        pool,
        &admin,
        role.role.id,
        Holder::User(submitter),
        &[Some(web)],
    )
    .await
    .unwrap();
    let submitter = access::load(pool, submitter).await.unwrap();

    let open = create(pool, &admin, web, "Open").await;
    let restricted = issues::create(
        pool,
        &member,
        WEB,
        NewIssue {
            audience: Some(AudienceInput {
                users: vec![other.user()],
                groups: vec![],
            }),
            ..new_issue(web, "Secret")
        },
    )
    .await
    .unwrap()
    .done
    .0;
    let own = create(pool, &submitter, web, "Mine").await;

    let ids =
        |d: issues::Described| -> Vec<IssueId> { d.issues.iter().map(|v| v.issue.id).collect() };
    let all = vec![own.issue.id, restricted.issue.id, open.issue.id];
    // Bypass: the system administrator sees everything (ISS-044).
    assert_eq!(ids(list(pool, &admin, web, 50).await), all);
    // The reporter and the listed account see the restricted one.
    assert_eq!(ids(list(pool, &member, web, 50).await), all);
    assert_eq!(ids(list(pool, &other, web, 50).await), all);
    // A member outside the audience does not, nor can open it.
    let third = person(pool, &admin, "tia", PredefinedRole::ProjectMember, &[web]).await;
    assert_eq!(
        ids(list(pool, &third, web, 50).await),
        [own.issue.id, open.issue.id]
    );
    assert!(matches!(
        issues::get(pool, &third, restricted.issue.id).await,
        Err(Error::NotFound(_))
    ));
    assert_eq!(count(pool, &third, web).await, (2, 2));
    // The submitter sees only their own issue (ACC-047, ACC-048).
    assert_eq!(ids(list(pool, &submitter, web, 50).await), [own.issue.id]);
    assert!(issues::get(pool, &submitter, own.issue.id).await.is_ok());
    assert!(issues::get(pool, &submitter, open.issue.id).await.is_err());
    // Pages: newest first, the cursor continues.
    let first = search::search(
        pool,
        &admin,
        SearchInput {
            text: "sort: id".to_owned(),
            project: Some(web),
            limit: 2,
            ..SearchInput::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(ids(first.issues), [own.issue.id, restricted.issue.id]);
    let page = search::search(
        pool,
        &admin,
        SearchInput {
            text: "sort: id".to_owned(),
            project: Some(web),
            cursor: first.next_cursor,
            limit: 2,
            ..SearchInput::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(ids(page.issues), [open.issue.id]);

    // Changing the audience is recorded (ISS-041); the list follows.
    issues::update(
        pool,
        &member,
        WEB,
        restricted.issue.id,
        IssueChange {
            audience: Some(AudienceInput::default()),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(ids(list(pool, &third, web, 50).await), all);
    assert!(
        kinds(pool, restricted.issue.id)
            .await
            .contains(&"visibility".to_owned())
    );
}

/// The list predicate in SQL and the rule in Rust agree (architecture.md §4).
#[tokio::test]
async fn acc_049_sql_and_rust_agree() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let group = groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "Insiders".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    let a = person(pool, &admin, "amy", PredefinedRole::ProjectMember, &[web]).await;
    let b = person(pool, &admin, "bob", PredefinedRole::ProjectMember, &[web]).await;
    groups::add_members(pool, &admin, group.id, &[b.user()])
        .await
        .unwrap();
    let b = access::load(pool, b.user()).await.unwrap();
    let c = person(pool, &admin, "cid", PredefinedRole::ProjectAdmin, &[web]).await;
    let audiences = [
        AudienceInput::default(),
        AudienceInput {
            users: vec![a.user()],
            groups: vec![],
        },
        AudienceInput {
            users: vec![],
            groups: vec![group.id],
        },
        AudienceInput {
            users: vec![c.user()],
            groups: vec![group.id],
        },
    ];
    let mut created = Vec::new();
    for (i, audience) in audiences.into_iter().enumerate() {
        for reporter in [&admin, &a, &c] {
            created.push(
                issues::create(
                    pool,
                    reporter,
                    WEB,
                    NewIssue {
                        audience: Some(audience.clone()),
                        ..new_issue(web, &format!("case {i}"))
                    },
                )
                .await
                .unwrap()
                .done
                .0,
            );
        }
    }
    for viewer in [&admin, &a, &b, &c] {
        let mut listed: Vec<IssueId> = list(pool, viewer, web, 100)
            .await
            .issues
            .iter()
            .map(|v| v.issue.id)
            .collect();
        listed.sort_unstable();
        let project = prostor_store::projects::get(pool, web)
            .await
            .unwrap()
            .unwrap();
        let mut ruled: Vec<IssueId> = created
            .iter()
            .filter(|v| issues::can_read(viewer, &v.issue, &project))
            .map(|v| v.issue.id)
            .collect();
        ruled.sort_unstable();
        assert_eq!(listed, ruled, "viewer {}", viewer.user());
        assert!(!viewer.has_in(Permission::VisibilityBypass, web) || listed.len() == created.len());
    }
}

#[tokio::test]
async fn fld_060_hist_007_private_fields_and_their_history() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Private").await;
    let estimation = field_id(&issue, "Estimation");
    project_fields::update(
        pool,
        &admin,
        web,
        estimation,
        project_fields::SettingsChange {
            private: Some(true),
            version: field(&issue, "Estimation").field.attached.version,
            ..project_fields::SettingsChange::default()
        },
    )
    .await
    .unwrap();
    set(
        pool,
        &admin,
        issue.issue.id,
        estimation,
        FieldChange::Set(Some(Input::Integer(60))),
    )
    .await
    .unwrap();
    // A reader without private read: the field and its history are hidden.
    let reader = create_user(pool, &admin, "rita").await;
    let role = roles::create(
        pool,
        &admin,
        roles::NewRole {
            name: "Reader".to_owned(),
            description: String::new(),
            permissions: ["project.basic", "issue.read", "issue.update"]
                .map(str::to_owned)
                .to_vec(),
        },
    )
    .await
    .unwrap();
    roles::assign(
        pool,
        &admin,
        role.role.id,
        Holder::User(reader),
        &[Some(web)],
    )
    .await
    .unwrap();
    let reader = access::load(pool, reader).await.unwrap();
    let view = issues::get(pool, &reader, issue.issue.id).await.unwrap().0;
    assert!(view.fields.iter().all(|f| f.field.id() != estimation));
    assert!(matches!(
        set(
            pool,
            &reader,
            issue.issue.id,
            estimation,
            FieldChange::Set(None)
        )
        .await,
        Err(Error::NotFound(_))
    ));
    let page = history(pool, &reader, issue.issue.id, None, Direction::Older, 50)
        .await
        .unwrap();
    assert!(page.entries.iter().all(|e| e.kind != "field"));
    let page = history(pool, &admin, issue.issue.id, None, Direction::Older, 50)
        .await
        .unwrap();
    assert_eq!(page.entries[0].kind, "field");
    assert_eq!(page.next, None);
    // A member reads and changes it.
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let view = issues::get(pool, &member, issue.issue.id).await.unwrap().0;
    assert!(field(&view, "Estimation").editable);
}

#[tokio::test]
async fn hist_010_history_pages_both_ways() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Pages").await;
    let estimation = field_id(&issue, "Estimation");
    for minutes in 1..=5 {
        set(
            pool,
            &admin,
            issue.issue.id,
            estimation,
            FieldChange::Set(Some(Input::Integer(minutes))),
        )
        .await
        .unwrap();
    }
    let first = history(pool, &admin, issue.issue.id, None, Direction::Older, 4)
        .await
        .unwrap();
    assert_eq!(first.entries.len(), 4);
    let rest = history(
        pool,
        &admin,
        issue.issue.id,
        first.next,
        Direction::Older,
        4,
    )
    .await
    .unwrap();
    assert_eq!(rest.entries.len(), 2);
    assert_eq!(rest.entries.last().unwrap().kind, "created");
    assert_eq!(rest.next, None);
    let newer = history(
        pool,
        &admin,
        issue.issue.id,
        Some(rest.entries[0].id),
        Direction::Newer,
        10,
    )
    .await
    .unwrap();
    assert_eq!(newer.entries.len(), 4);
    assert!(newer.entries.windows(2).all(|w| w[0].id < w[1].id));
    assert_eq!(newer.entries[0].author_id, admin.user());
    assert_eq!(newer.entries[0].origin, "web");
}

#[tokio::test]
async fn iss_025_fld_059_moves_keep_drop_and_reset_values() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let app = project(pool, &admin, "App", "APP").await;
    let issue = create(pool, &admin, web, "Mover").await;
    let id = issue.issue.id;
    // App gets its own copy of the priorities without "High"; Web's issue holds "High".
    let priority = field_id(&issue, "Priority");
    let high = values_of(pool, &issue, "Priority")
        .await
        .into_iter()
        .find(|v| v.name == "High")
        .unwrap();
    set(
        pool,
        &admin,
        id,
        priority,
        FieldChange::Set(Some(Input::Integer(high.id))),
    )
    .await
    .unwrap();
    project_fields::fork_set(pool, &admin, app, priority)
        .await
        .unwrap();
    let app_set =
        prostor_store::fields::lock_attached(&mut pool.acquire().await.unwrap(), app, priority)
            .await
            .unwrap()
            .unwrap()
            .set_id
            .unwrap();
    let copy_of_high = prostor_store::value_sets::values_of(pool, &[app_set])
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.name == "High")
        .unwrap();
    value_sets::delete_value(pool, &admin, app_set, copy_of_high.id, None)
        .await
        .unwrap();
    // Estimation is not in App.
    let estimation = field_id(&issue, "Estimation");
    set(
        pool,
        &admin,
        id,
        estimation,
        FieldChange::Set(Some(Input::Integer(30))),
    )
    .await
    .unwrap();
    project_fields::detach(pool, &admin, app, estimation)
        .await
        .unwrap();

    let preview = issues::moves::preview(pool, &admin, id, app).await.unwrap();
    let mut changed: Vec<(String, String)> = preview
        .changes
        .iter()
        .map(|(f, _, outcome)| (f.field.name.clone(), format!("{outcome:?}")))
        .collect();
    changed.sort();
    assert_eq!(changed.len(), 2, "{changed:?}");
    assert!(changed[0].0 == "Estimation" && changed[0].1 == "Erased");
    assert!(changed[1].0 == "Priority" && changed[1].1.starts_with("Replaced(Some"));
    assert!(!preview.loses_access);

    let moved = issues::moves::move_issue(pool, &admin, WEB, id, app, false)
        .await
        .unwrap()
        .done
        .unwrap()
        .0;
    let app_normal = prostor_store::value_sets::values_of(pool, &[app_set])
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.name == "Normal")
        .unwrap();
    assert_eq!(
        field(&moved, "Priority").value,
        Some(Value::Refs(vec![app_normal.id]))
    );
    assert!(
        moved
            .fields
            .iter()
            .all(|f| f.field.field.name != "Estimation")
    );
    // Moving back does not bring the dropped value back (FLD-059).
    let back = issues::moves::move_issue(pool, &admin, WEB, id, web, false)
        .await
        .unwrap()
        .done
        .unwrap()
        .0;
    assert!(field(&back, "Estimation").value.is_none());
    assert_eq!(back.readable_id().as_deref(), Some("WEB-2"));
    // Moving needs update where it is and create where it goes.
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    assert!(matches!(
        issues::moves::move_issue(pool, &member, WEB, id, app, false).await,
        Err(Error::NotFound(_) | Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn iss_030_iss_032_stars_and_votes() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let issue = create(pool, &admin, web, "Popular").await;
    let id = issue.issue.id;
    // The creator watches from publication (ISS-030's auto-watch, plan 1.12 §7.1).
    assert!(issues::get(pool, &admin, id).await.unwrap().0.starred);
    issues::set_star(pool, &member, id, true).await.unwrap();
    issues::set_star(pool, &member, id, true).await.unwrap();
    let view = issues::get(pool, &member, id).await.unwrap().0;
    assert!(view.starred);
    assert_eq!(view.watchers, Some(2));
    // The star is one's own: taking it off leaves the creator's auto-watch standing.
    issues::set_star(pool, &member, id, false).await.unwrap();
    let view = issues::get(pool, &member, id).await.unwrap().0;
    assert!(!view.starred);
    assert_eq!(view.watchers, Some(1));
    issues::set_vote(pool, &member, id, true).await.unwrap();
    issues::set_vote(pool, &member, id, true).await.unwrap();
    assert!(matches!(
        issues::set_vote(pool, &admin, id, true).await,
        Err(Error::Conflict(_))
    ));
    let view = issues::get(pool, &member, id).await.unwrap().0;
    assert!(view.voted);
    assert_eq!(view.issue.votes, 1);
    issues::set_vote(pool, &member, id, false).await.unwrap();
    assert_eq!(
        issues::get(pool, &member, id).await.unwrap().0.issue.votes,
        0
    );
    // Watchers on creation: others need the watcher permission (ISS-007).
    let reader = create_user(pool, &admin, "rita").await;
    let created = issues::create(
        pool,
        &member,
        WEB,
        NewIssue {
            watchers: vec![member.user(), admin.user()],
            ..new_issue(web, "Watched")
        },
    )
    .await
    .unwrap()
    .done
    .0;
    assert_eq!(created.watchers, Some(2));
    let _ = reader;
}

#[tokio::test]
async fn iss_034_iss_035_iss_036_personal_tags() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let issue = create(pool, &admin, web, "Tagged").await;
    let id = issue.issue.id;
    let later = tags::create(pool, &member, " later ", Some(3))
        .await
        .unwrap();
    assert_eq!(later.name, "later");
    assert!(matches!(
        tags::create(pool, &member, "LATER", None).await,
        Err(Error::Invalid(_))
    ));
    let mine = tags::create(pool, &admin, "mine", None).await.unwrap();
    tags::set_on_issue(pool, &member, WEB, id, later.id, true)
        .await
        .unwrap();
    tags::set_on_issue(pool, &admin, WEB, id, mine.id, true)
        .await
        .unwrap();
    // Somebody else's tag cannot be used.
    assert!(matches!(
        tags::set_on_issue(pool, &member, WEB, id, mine.id, true).await,
        Err(Error::NotFound(_))
    ));
    // Each sees their own tags, and only theirs in the history (HIST-007).
    let seen = issues::get(pool, &member, id).await.unwrap().0;
    assert_eq!(
        seen.tags
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["later"]
    );
    let page = history(pool, &member, id, None, Direction::Older, 50)
        .await
        .unwrap();
    let tag_records: Vec<_> = page.entries.iter().filter(|e| e.kind == "tags").collect();
    assert_eq!(tag_records.len(), 1);
    assert_eq!(tag_records[0].added, Some(json!([later.id])));
    assert_eq!(
        tags::list(pool, &member, Listing::Mine, "lat", None, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    // Renaming and deleting (ISS-036).
    let renamed = tags::update(
        pool,
        &member,
        later.id,
        tags::TagChange {
            name: Some("soon".to_owned()),
            color: Some(None),
            version: later.version,
            ..tags::TagChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!((renamed.name.as_str(), renamed.color), ("soon", None));
    assert_eq!(tags::usage(pool, &member, later.id).await.unwrap(), 1);
    assert!(matches!(
        tags::delete(pool, &member, mine.id).await,
        Err(Error::NotFound(_))
    ));
    tags::delete(pool, &member, later.id).await.unwrap();
    assert!(
        issues::get(pool, &member, id)
            .await
            .unwrap()
            .0
            .tags
            .is_empty()
    );
}

#[tokio::test]
async fn iss_028_deleted_issues_disappear() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let reporter = create_user(pool, &admin, "sam").await;
    let role = roles::create(
        pool,
        &admin,
        roles::NewRole {
            name: "Submitter".to_owned(),
            description: String::new(),
            permissions: ["project.basic", "issue.create"]
                .map(str::to_owned)
                .to_vec(),
        },
    )
    .await
    .unwrap();
    roles::assign(
        pool,
        &admin,
        role.role.id,
        Holder::User(reporter),
        &[Some(web)],
    )
    .await
    .unwrap();
    let reporter = access::load(pool, reporter).await.unwrap();
    let issue = create(pool, &reporter, web, "Gone").await;
    // The reporter edits but does not delete without the permission (ISS-022).
    assert!(matches!(
        issues::delete(pool, &reporter, WEB, issue.issue.id).await,
        Err(Error::Forbidden(_))
    ));
    issues::delete(pool, &admin, WEB, issue.issue.id)
        .await
        .unwrap();
    assert!(matches!(
        issues::get(pool, &admin, issue.issue.id).await,
        Err(Error::NotFound(_))
    ));
    assert!(list(pool, &admin, web, 50).await.issues.is_empty());
    // It stays whole for the trash.
    let row = prostor_store::issues::get(pool, issue.issue.id)
        .await
        .unwrap()
        .unwrap();
    assert!(row.deleted_at.is_some());
    assert_eq!(kinds(pool, issue.issue.id).await, ["created", "deleted"]);
}

#[tokio::test]
async fn fld_032_fld_046_value_changes_reach_issues_in_the_background() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Values").await;
    let id = issue.issue.id;
    let priorities = values_of(pool, &issue, "Priority").await;
    let set_id = priorities[0].set_id;
    let high = priorities.iter().find(|v| v.name == "High").unwrap();
    let low = priorities.iter().find(|v| v.name == "Low").unwrap();
    let priority = field_id(&issue, "Priority");
    set(
        pool,
        &admin,
        id,
        priority,
        FieldChange::Set(Some(Input::Integer(high.id))),
    )
    .await
    .unwrap();
    assert_eq!(
        value_sets::value_usage(pool, &admin, set_id, high.id)
            .await
            .unwrap(),
        1
    );
    value_sets::delete_value(pool, &admin, set_id, high.id, Some(low.id))
        .await
        .unwrap();
    jobs::run(
        pool,
        FieldWork::Replace {
            value: high.id,
            name: "High".to_owned(),
            replacement: Some(low.id),
            actor: admin.user(),
        },
    )
    .await
    .unwrap();
    let view = issues::get(pool, &admin, id).await.unwrap().0;
    assert_eq!(
        field(&view, "Priority").value,
        Some(Value::Refs(vec![low.id]))
    );
    let records = prostor_store::issue_history::all_of(pool, id)
        .await
        .unwrap();
    let last = records.last().unwrap();
    assert_eq!(
        last.removed,
        Some(json!([{ "id": high.id, "name": "High" }]))
    );
    assert_eq!(last.origin, "system");
    assert!(last.silent);

    // A state value flagged resolved resolves the issues holding it (FLD-046, ISS-023).
    let state = field(&view, "State");
    let Some(Value::Refs(held)) = &state.value else {
        panic!("no state")
    };
    let current = values_of(pool, &view, "State")
        .await
        .into_iter()
        .find(|v| v.id == held[0])
        .unwrap();
    assert!(!current.resolved);
    value_sets::update_value(
        pool,
        &admin,
        current.set_id,
        current.id,
        value_sets::ValueInput {
            resolved: Some(true),
            ..value_sets::ValueInput::default()
        },
        current.version,
    )
    .await
    .unwrap();
    jobs::run(
        pool,
        FieldWork::ResolveValue {
            value: current.id,
            actor: admin.user(),
        },
    )
    .await
    .unwrap();
    let view = issues::get(pool, &admin, id).await.unwrap().0;
    assert!(view.issue.resolved_at.is_some());
    assert_eq!(kinds(pool, id).await.last().unwrap(), "resolved");
}

#[tokio::test]
async fn fld_023_fld_054_detaching_and_deleting_fields_erase_values() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let app = project(pool, &admin, "App", "APP").await;
    let a = create(pool, &admin, web, "A").await;
    let b = create(pool, &admin, app, "B").await;
    let estimation = field_id(&a, "Estimation");
    for issue in [&a, &b] {
        set(
            pool,
            &admin,
            issue.issue.id,
            estimation,
            FieldChange::Set(Some(Input::Integer(15))),
        )
        .await
        .unwrap();
    }
    assert_eq!(
        prostor_service::fields::deletion_impact(pool, &admin, estimation)
            .await
            .unwrap(),
        (2, 2, 0)
    );
    assert_eq!(
        project_fields::detach_impact(pool, &admin, web, estimation)
            .await
            .unwrap(),
        1
    );
    project_fields::detach(pool, &admin, web, estimation)
        .await
        .unwrap();
    // Attaching it again brings nothing back.
    project_fields::attach(
        pool,
        &admin,
        web,
        project_fields::AttachInput::Existing {
            field: estimation,
            set: project_fields::SetUse::Shared,
        },
    )
    .await
    .unwrap();
    let view = issues::get(pool, &admin, a.issue.id).await.unwrap().0;
    assert!(field(&view, "Estimation").value.is_none());
    prostor_service::fields::delete(pool, &admin, estimation)
        .await
        .unwrap();
    jobs::run(
        pool,
        FieldWork::Erase {
            field: estimation,
            kind: FieldKind::Duration.key().to_owned(),
        },
    )
    .await
    .unwrap();
    let rows =
        prostor_store::issue_values::of_issues(&mut pool.acquire().await.unwrap(), &[b.issue.id])
            .await
            .unwrap();
    assert!(rows.iter().all(|r| r.field != estimation));
}

#[tokio::test]
async fn acc_006_acc_032_deleted_accounts_and_groups_in_issues() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let leaver = person(pool, &admin, "leo", PredefinedRole::ProjectMember, &[web]).await;
    let heir = person(pool, &admin, "hal", PredefinedRole::ProjectMember, &[web]).await;
    let group = groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "Old".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    let new_group = groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "New".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    let issue = issues::create(
        pool,
        &leaver,
        WEB,
        NewIssue {
            audience: Some(AudienceInput {
                users: vec![leaver.user()],
                groups: vec![group.id],
            }),
            ..new_issue(web, "Inherited")
        },
    )
    .await
    .unwrap()
    .done
    .0;
    let id = issue.issue.id;
    let assignee = field_id(&issue, "Assignee");
    prostor_service::teams::add(pool, &admin, web, &[leaver.user()], &[])
        .await
        .unwrap();
    set(
        pool,
        &admin,
        id,
        assignee,
        FieldChange::Set(Some(Input::Integer(leaver.user()))),
    )
    .await
    .unwrap();
    issues::set_star(pool, &leaver, id, true).await.unwrap();
    tags::create(pool, &leaver, "mine", None).await.unwrap();

    users::delete(pool, &admin, &[leaver.user()], heir.user())
        .await
        .unwrap();
    groups::delete(pool, &admin, &[group.id], new_group.id)
        .await
        .unwrap();
    let row = prostor_store::issues::get(pool, id).await.unwrap().unwrap();
    assert_eq!(row.reporter_id, leaver.user());
    assert_eq!(row.audience_users, [heir.user()]);
    assert_eq!(row.audience_groups, [new_group.id]);
    // The assignee can be empty, so the value goes (ACC-006). The deleted account's watcher
    // row goes with it — here that was the creator's auto-watch too (ISS-030).
    let view = issues::get(pool, &admin, id).await.unwrap().0;
    assert!(field(&view, "Assignee").value.is_none());
    assert_eq!(view.watchers, Some(0));
}

#[tokio::test]
async fn prj_064_deleting_a_project_removes_its_issues() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let app = project(pool, &admin, "App", "APP").await;
    let issue = create(pool, &admin, web, "One").await;
    create(pool, &admin, web, "Two").await;
    // An issue moved away keeps living elsewhere.
    let moved = issues::moves::move_issue(pool, &admin, WEB, issue.issue.id, app, false)
        .await
        .unwrap()
        .done
        .unwrap()
        .0;
    projects::delete(pool, &admin, web).await.unwrap();
    projects::run_deletion(pool, projects::ProjectDeletion { project: web })
        .await
        .unwrap();
    assert!(
        prostor_store::projects::get(pool, web)
            .await
            .unwrap()
            .is_none()
    );
    assert!(issues::get(pool, &admin, moved.issue.id).await.is_ok());
    assert_eq!(
        prostor_store::issues::count_of_project(pool, web)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn prj_039_prj_061_default_audience_and_archived_projects() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let group = groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "Insiders".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    let version = projects::get(pool, &admin, web)
        .await
        .unwrap()
        .project
        .version;
    projects::update(
        pool,
        &admin,
        web,
        ProjectChange {
            audience: Some(Some(group.id)),
            version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap();
    let issue = create(pool, &admin, web, "Default audience").await;
    assert_eq!(issue.issue.audience_groups, [group.id]);
    projects::set_archived(pool, &admin, web, true)
        .await
        .unwrap();
    assert!(issues::get(pool, &admin, issue.issue.id).await.is_ok());
    assert!(matches!(
        issues::create(pool, &admin, WEB, new_issue(web, "No")).await,
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        issues::set_star(pool, &admin, issue.issue.id, true).await,
        Ok(())
    ));
    let estimation = field_id(&issue, "Estimation");
    assert!(matches!(
        set(
            pool,
            &admin,
            issue.issue.id,
            estimation,
            FieldChange::Set(None)
        )
        .await,
        Err(Error::Conflict(_))
    ));
}

#[tokio::test]
async fn iss_043_audience_candidates() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = create_user(pool, &admin, "mia").await;
    prostor_service::teams::add(pool, &admin, web, &[member], &[])
        .await
        .unwrap();
    let candidates = issues::audience::candidates(pool, &admin, web, "")
        .await
        .unwrap();
    assert!(candidates.groups[0].team_of == Some(web));
    assert!(candidates.users.iter().any(|u| u.id == member));
    let filtered = issues::audience::candidates(pool, &admin, web, "mia")
        .await
        .unwrap();
    assert_eq!(filtered.users.len(), 1);
    let _ = FieldInput {
        field: 0,
        value: None,
    };
}

async fn draft(pool: &PgPool, actor: &Access, project: ProjectId, summary: &str) -> IssueId {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            publish: false,
            ..new_issue(project, summary)
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id
}

fn ids_of(d: &issues::Described) -> Vec<IssueId> {
    d.issues.iter().map(|v| v.issue.id).collect()
}

#[tokio::test]
async fn iss_001_batch_keeps_order_and_drops_unreadable() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let ops = project(pool, &admin, "Ops", "OPS").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let a = create(pool, &admin, web, "A").await.issue.id;
    let b = create(pool, &admin, web, "B").await.issue.id;
    let hidden = create(pool, &admin, ops, "Hidden").await.issue.id;
    let mine = draft(pool, &member, web, "Mine").await;
    let theirs = draft(pool, &admin, web, "Theirs").await;
    let found = issues::describe_many(pool, &member, &[b, hidden, 999_999, a, mine, theirs, a])
        .await
        .unwrap();
    assert_eq!(ids_of(&found), [b, a, mine]);
    // The lookups come with them, as for one issue.
    assert!(found.users.iter().any(|u| u.id == admin.user()));
}

#[tokio::test]
async fn iss_001_batch_limit() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let err = issues::describe_many(pool, &admin, &[]).await.unwrap_err();
    assert_eq!(problems(err), [("ids".to_owned(), Problem::Required)]);
    let many: Vec<IssueId> = (1..=101).collect();
    let err = issues::describe_many(pool, &admin, &many)
        .await
        .unwrap_err();
    assert_eq!(problems(err), [("ids".to_owned(), Problem::TooLong)]);
    let found = issues::describe_many(pool, &admin, &many[..100])
        .await
        .unwrap();
    assert!(found.issues.is_empty());
}

async fn options(
    pool: &PgPool,
    viewer: &Access,
    issue: IssueId,
    field: FieldId,
    q: &str,
    limit: i64,
) -> Result<issues::options::FieldOptions, Error> {
    issues::options::field_options(pool, viewer, issue, field, q, limit).await
}

#[tokio::test]
async fn fld_001_options_values_without_archived_in_set_order() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Options").await;
    let priority = field_id(&issue, "Priority");
    let set = field(&issue, "Priority").field.attached.set_id.unwrap();
    let all = values_of(pool, &issue, "Priority").await;
    assert!(all.len() >= 3);
    let gone = &all[0];
    value_sets::update_value(
        pool,
        &admin,
        set,
        gone.id,
        value_sets::ValueInput {
            archived: Some(true),
            ..value_sets::ValueInput::default()
        },
        gone.version,
    )
    .await
    .unwrap();
    let found = options(pool, &admin, issue.issue.id, priority, "", 20)
        .await
        .unwrap();
    let names: Vec<&str> = found.values.iter().map(|v| v.name.as_str()).collect();
    let expected: Vec<&str> = all[1..].iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, expected);
    assert!(!found.more);
    assert!(found.users.is_empty() && found.groups.is_empty());
    // `q` matches part of a name, ignoring case; `limit` cuts and says there is more.
    let part: String = all[1].name.chars().skip(1).take(3).collect();
    let found = options(
        pool,
        &admin,
        issue.issue.id,
        priority,
        &part.to_uppercase(),
        20,
    )
    .await
    .unwrap();
    assert!(found.values.iter().any(|v| v.id == all[1].id));
    let found = options(pool, &admin, issue.issue.id, priority, "", 1)
        .await
        .unwrap();
    assert_eq!(found.values.len(), 1);
    assert!(found.more);
}

#[tokio::test]
async fn fld_001_options_people_with_groups_expanded() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "People").await;
    let assignee = field_id(&issue, "Assignee");
    let set = field(&issue, "Assignee").field.attached.set_id.unwrap();
    let zed = person(pool, &admin, "zed", PredefinedRole::ProjectMember, &[web]).await;
    let yan = create_user(pool, &admin, "yan").await;
    let group = groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "Designers".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    groups::add_members(pool, &admin, group.id, &[yan, zed.user()])
        .await
        .unwrap();
    value_sets::add_members(pool, &admin, set, &[admin.user()], &[group.id])
        .await
        .unwrap();
    let names = |o: &issues::options::FieldOptions| -> Vec<String> {
        o.users.iter().map(|u| u.username.clone()).collect()
    };
    // Listed accounts and members of listed groups, by name, the viewer first.
    let found = options(pool, &zed, issue.issue.id, assignee, "", 20)
        .await
        .unwrap();
    let seen = names(&found);
    assert_eq!(seen.first().map(String::as_str), Some("zed"));
    assert!(seen.contains(&"yan".to_owned()) && seen.contains(&"admin".to_owned()));
    let found = options(pool, &zed, issue.issue.id, assignee, "YAN p", 20)
        .await
        .unwrap();
    assert_eq!(names(&found), ["yan"]);
    // A user set takes no new values from the picker.
    assert!(!found.can_add_value);
}

#[tokio::test]
async fn fld_001_options_groups_and_private_fields() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let created = prostor_service::fields::create(
        pool,
        &admin,
        prostor_service::fields::NewFieldInput {
            name: "Team".to_owned(),
            kind: FieldKind::Group,
            multi: false,
            aliases: Vec::new(),
            set: prostor_service::fields::SetSource::New { name: None },
            can_be_empty: true,
            empty_label: String::new(),
            private: false,
            default_values: Vec::new(),
            auto_attach: true,
        },
    )
    .await
    .unwrap();
    groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "Designers".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    // Auto-attached fields join projects created later.
    let ops = project(pool, &admin, "Ops", "OPS").await;
    let in_ops = create(pool, &admin, ops, "Groups").await;
    let found = options(pool, &admin, in_ops.issue.id, created.field.id, "desig", 20)
        .await
        .unwrap();
    let names: Vec<&str> = found.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["Designers"]);
    let issue = create(pool, &admin, web, "Private").await;
    // A private field is not found for someone who may not update it.
    let estimation = field_id(&issue, "Estimation");
    project_fields::update(
        pool,
        &admin,
        web,
        estimation,
        project_fields::SettingsChange {
            private: Some(true),
            version: field(&issue, "Estimation").field.attached.version,
            ..project_fields::SettingsChange::default()
        },
    )
    .await
    .unwrap();
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let private_update = prostor_domain::issues::rights::may_update_private(&member, web).is_ok();
    let answer = options(pool, &member, issue.issue.id, estimation, "", 20).await;
    assert_eq!(answer.is_ok(), private_update);
    assert!(
        options(pool, &admin, issue.issue.id, estimation, "", 20)
            .await
            .is_ok()
    );
    // A field the project lacks, and an issue the viewer cannot read, are not found.
    assert!(matches!(
        options(pool, &admin, issue.issue.id, 999_999, "", 20).await,
        Err(Error::NotFound(_))
    ));
    let outsider = create_user(pool, &admin, "otto").await;
    let outsider = access::load(pool, outsider).await.unwrap();
    assert!(matches!(
        options(pool, &outsider, in_ops.issue.id, created.field.id, "", 20).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn fld_001_options_can_add_value_needs_every_project_of_the_set() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Add").await;
    let priority = field_id(&issue, "Priority");
    let set = field(&issue, "Priority").field.attached.set_id.unwrap();
    let lead = person(pool, &admin, "lea", PredefinedRole::ProjectAdmin, &[web]).await;
    let found = options(pool, &lead, issue.issue.id, priority, "", 20)
        .await
        .unwrap();
    let only_web = prostor_store::value_sets::usage(pool, set)
        .await
        .unwrap()
        .projects
        == [web];
    assert_eq!(found.can_add_value, only_web);
    // Shared with a project the lead cannot configure: no new values from the picker.
    let ops = project(pool, &admin, "Ops", "OPS").await;
    let in_ops = prostor_store::value_sets::usage(pool, set)
        .await
        .unwrap()
        .projects
        .contains(&ops);
    if !in_ops {
        project_fields::attach(
            pool,
            &admin,
            ops,
            project_fields::AttachInput::Existing {
                field: priority,
                set: project_fields::SetUse::Shared,
            },
        )
        .await
        .unwrap();
    }
    let found = options(pool, &lead, issue.issue.id, priority, "", 20)
        .await
        .unwrap();
    assert!(!found.can_add_value);
    let found = options(pool, &admin, issue.issue.id, priority, "", 20)
        .await
        .unwrap();
    assert!(found.can_add_value);
}

#[tokio::test]
async fn iss_011_delete_all_drafts_only_own() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    draft(pool, &member, web, "One").await;
    draft(pool, &member, web, "").await;
    let kept = draft(pool, &admin, web, "Admin's").await;
    let published = create(pool, &member, web, "Published").await.issue.id;
    issues::delete_drafts(pool, &member).await.unwrap();
    assert!(
        issues::drafts(pool, &member)
            .await
            .unwrap()
            .issues
            .is_empty()
    );
    assert_eq!(ids_of(&issues::drafts(pool, &admin).await.unwrap()), [kept]);
    assert!(issues::get(pool, &member, published).await.is_ok());
    // Nothing to delete is fine.
    issues::delete_drafts(pool, &member).await.unwrap();
}

#[tokio::test]
async fn iss_010_draft_may_be_empty() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let id = draft(pool, &admin, web, "  ").await;
    let view = issues::get(pool, &admin, id).await.unwrap().0;
    assert_eq!(view.issue.summary, "");
    assert_eq!(view.issue.description, "");
    let err = issues::publish(pool, &admin, WEB, id).await.unwrap_err();
    assert_eq!(problems(err), [("summary".to_owned(), Problem::Required)]);
}

#[tokio::test]
async fn iss_010_sweep_keeps_drafts_with_content() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let empty_old = draft(pool, &admin, web, "").await;
    let empty_fresh = draft(pool, &admin, web, "").await;
    let titled = draft(pool, &admin, web, "Titled").await;
    let described = issues::create(
        pool,
        &admin,
        WEB,
        NewIssue {
            publish: false,
            description: "Only a description".to_owned(),
            ..new_issue(web, "")
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;
    let tagged = draft(pool, &admin, web, "").await;
    let tag = tags::create(pool, &admin, "later", None).await.unwrap();
    tags::set_on_issue(pool, &admin, WEB, tagged, tag.id, true)
        .await
        .unwrap();
    let published = create(pool, &admin, web, "Published").await.issue.id;
    let old = [empty_old, titled, described, tagged, published];
    sqlx::query("update issues set updated_at = now() - interval '25 hours' where id = any($1)")
        .bind(&old[..])
        .execute(pool)
        .await
        .unwrap();
    let swept = issues::sweep_drafts(pool, issues::EMPTY_DRAFT_IDLE)
        .await
        .unwrap();
    assert_eq!(swept, 1);
    let mut left = ids_of(&issues::drafts(pool, &admin).await.unwrap());
    left.sort_unstable();
    assert_eq!(left, [empty_fresh, titled, described, tagged]);
    assert!(issues::get(pool, &admin, published).await.is_ok());
}

#[tokio::test]
async fn qry_078_fields_of_readable_projects() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let ops = project(pool, &admin, "Ops", "OPS").await;
    let issue = create(pool, &admin, web, "Private").await;
    let estimation = field_id(&issue, "Estimation");
    project_fields::update(
        pool,
        &admin,
        web,
        estimation,
        project_fields::SettingsChange {
            private: Some(true),
            version: field(&issue, "Estimation").field.attached.version,
            ..project_fields::SettingsChange::default()
        },
    )
    .await
    .unwrap();
    let customer = project_fields::attach(
        pool,
        &admin,
        ops,
        project_fields::AttachInput::New {
            name: "Customer".to_owned(),
            kind: FieldKind::Enum,
            multi: false,
            aliases: Vec::new(),
        },
    )
    .await
    .unwrap()
    .field
    .id;
    let names = |found: &[(prostor_store::fields::Field, i64)]| -> Vec<String> {
        found.iter().map(|(f, _)| f.name.clone()).collect()
    };
    // The admin: every field, the one-project field last (used by one of two projects).
    let all = fields::of_readable(pool, &admin, None).await.unwrap();
    assert_eq!(
        all.last().map(|(f, uses)| (f.id, *uses)),
        Some((customer, 1))
    );
    assert!(all.iter().any(|(f, uses)| f.id == estimation && *uses == 2));
    let only_web = fields::of_readable(pool, &admin, Some(web)).await.unwrap();
    assert!(!names(&only_web).contains(&"Customer".to_owned()));
    // A reader of WEB without private read: no OPS field, no private Estimation.
    let reader = create_user(pool, &admin, "rita").await;
    let role = roles::create(
        pool,
        &admin,
        roles::NewRole {
            name: "Reader".to_owned(),
            description: String::new(),
            permissions: ["project.basic", "issue.read"].map(str::to_owned).to_vec(),
        },
    )
    .await
    .unwrap();
    roles::assign(
        pool,
        &admin,
        role.role.id,
        Holder::User(reader),
        &[Some(web)],
    )
    .await
    .unwrap();
    let reader = access::load(pool, reader).await.unwrap();
    let seen = fields::of_readable(pool, &reader, None).await.unwrap();
    assert!(!seen.is_empty());
    assert!(
        seen.iter()
            .all(|(f, uses)| f.id != customer && f.id != estimation && *uses == 1)
    );
    assert_eq!(
        names(&seen),
        names(&fields::of_readable(pool, &reader, Some(web)).await.unwrap())
    );
    assert!(matches!(
        fields::of_readable(pool, &reader, Some(ops)).await,
        Err(Error::NotFound(_))
    ));
}
