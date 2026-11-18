//! The engine of the built-in behaviors (AUT-023, step 1.9a-2): rule writes mark their origin,
//! the focus issue's records join the triggering change, messages ride the answers, a project
//! without rows runs nothing, the recursion is bounded and a disabled rule does nothing. The
//! behaviors themselves land with parts 1.9a-3…1.9a-5.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::access::Access;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::issues::history::{Origin, Record};
use taskmaster_domain::issues::values::{Input, Value};
use taskmaster_domain::projects::Template;
use taskmaster_domain::rules::Rule;
use taskmaster_domain::settings::Language;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{FieldId, IssueId, ProjectId, UserId};
use taskmaster_service::access;
use taskmaster_service::auth::SignedIn;
use taskmaster_service::comments::{self, NewComment};
use taskmaster_service::issues::{
    self, AudienceInput, FieldChange, IssueChange, IssueView, NewIssue,
};
use taskmaster_service::live::Batch;
use taskmaster_service::projects::{self, NewProjectInput};
use taskmaster_service::roles;
use taskmaster_service::rules::{self, Ctx, Event, MAX_DEPTH, MAX_WRITES};
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::users::{self, NewAccount};
use taskmaster_service::value_sets::{self, ValueInput};
use taskmaster_service::{Error, Passwords};
use taskmaster_store::PgPool;
use taskmaster_store::access::Holder;
use taskmaster_store::issue_history;
use taskmaster_store::sessions::Origin as SessionOrigin;
use taskmaster_testkit::TestDb;
use time::macros::datetime;

const T0: time::OffsetDateTime = datetime!(2026-11-05 09:00 UTC);
const WEB: Origin = Origin::Web;

use std::collections::BTreeMap;

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

/// A blank project: its template turns the eight rules of PRJ-022 on (AUT-022).
async fn project(pool: &PgPool, actor: &Access, key: &str) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Blank,
            name: key.to_owned(),
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

async fn create(pool: &PgPool, actor: &Access, project: ProjectId, summary: &str) -> IssueView {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: summary.to_owned(),
            publish: true,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
}

fn state_field(view: &IssueView) -> FieldId {
    view.fields
        .iter()
        .find(|f| f.field.kind.kind == FieldKind::State)
        .unwrap()
        .field
        .id()
}

/// The project's state field and the ID of its first resolved value («Fixed» in the default set).
async fn state_with_resolved(
    conn: &mut sqlx::PgConnection,
    project: ProjectId,
) -> (taskmaster_service::issues::values::ProjectField, i64) {
    let fields = taskmaster_service::issues::values::of_project(conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.kind.kind == FieldKind::State)
        .unwrap()
        .clone();
    let set = field.attached.set_id.unwrap();
    let values = taskmaster_store::value_sets::values_of(conn, &[set])
        .await
        .unwrap();
    let resolved = values.iter().find(|v| v.resolved).unwrap();
    (field, resolved.id)
}

/// The origins and authors of an issue's history, oldest first.
async fn history(pool: &PgPool, issue: IssueId) -> Vec<(String, UserId)> {
    issue_history::all_of(pool, issue)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| (entry.origin, entry.author_id))
        .collect()
}

fn kinds(records: &[Record]) -> Vec<&'static str> {
    records.iter().map(|r| r.kind.key()).collect()
}

/// A rule write carries the rule as its origin and the triggering actor as its author (§3.12).
#[tokio::test]
async fn aut_023_rule_write_marks_origin() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;
    let id = view.issue.id;

    let mut conn = pool.acquire().await.unwrap();
    let (field, resolved) = state_with_resolved(&mut conn, web).await;
    let row = taskmaster_store::issues::get(&mut *conn, id)
        .await
        .unwrap()
        .unwrap();
    let mut batch = Batch::new();
    let mut ctx = Ctx::new(&admin);
    let records = rules::write(
        &mut conn,
        &mut batch,
        &mut ctx,
        &row,
        &field,
        Some(&Value::Refs(vec![resolved])),
        Rule::FixedInBuild,
    )
    .await
    .unwrap();
    drop(conn);

    // The field record, and the resolution settling after it.
    assert_eq!(kinds(&records), ["field", "resolved"]);
    // History: creation by the actor, then the rule's records under the same person.
    let entries = history(pool, id).await;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], ("web".to_owned(), admin.user()));
    assert_eq!(entries[1], ("rule".to_owned(), admin.user()));
    assert_eq!(entries[2], ("rule".to_owned(), admin.user()));
    // The issue was touched by the write.
    let after = issues::get(pool, &admin, id).await.unwrap().0;
    assert_eq!(after.issue.updated_by, Some(admin.user()));
}

/// A rule write on the focus issue defers its records to the caller, so they join the change
/// that triggered them as one change (§3.5) — the mechanics the entry points rely on.
#[tokio::test]
async fn aut_023_records_join_the_change() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;
    let id = view.issue.id;

    let mut conn = pool.acquire().await.unwrap();
    let (field, resolved) = state_with_resolved(&mut conn, web).await;
    let row = taskmaster_store::issues::get(&mut *conn, id)
        .await
        .unwrap()
        .unwrap();
    let mut batch = Batch::new();
    let mut ctx = Ctx::new(&admin);
    ctx.focus = Some(id);
    let records = rules::write(
        &mut conn,
        &mut batch,
        &mut ctx,
        &row,
        &field,
        Some(&Value::Refs(vec![resolved])),
        Rule::FixedInBuild,
    )
    .await
    .unwrap();
    // Deferred: nothing is written until the caller says so.
    assert_eq!(kinds(&records), ["field", "resolved"]);
    assert_eq!(history(pool, id).await.len(), 1);
    // The caller writes the records after its own, in the rule's name: one moment of history.
    rules::finish(&mut conn, &mut batch, &ctx, &row, &records)
        .await
        .unwrap();
    drop(conn);
    let entries = history(pool, id).await;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1], ("rule".to_owned(), admin.user()));
    assert_eq!(entries[2], ("rule".to_owned(), admin.user()));
}

/// The answers of the use cases that can fire behaviors carry their messages (§3.11).
#[tokio::test]
async fn aut_023_messages_in_answer() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;

    let mut conn = pool.acquire().await.unwrap();
    let (_, resolved) = state_with_resolved(&mut conn, web).await;
    drop(conn);
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        state_field(&view),
        FieldChange::Set(Some(Input::Integer(resolved))),
        false,
    )
    .await
    .unwrap();
    // No behavior exists yet (they land with 1.9a-3…5): the field is there and stays empty.
    assert!(changed.messages.is_empty());
    let state = changed
        .done
        .0
        .fields
        .iter()
        .find(|f| f.field.kind.kind == FieldKind::State)
        .unwrap();
    assert_eq!(state.value, Some(Value::Refs(vec![resolved])));
    // The issue's answer is complete: the state really resolved.
    assert!(changed.done.0.issue.resolved_at.is_some());
}

/// A project without rule rows runs nothing: `on` reads the toggles once and answers no records.
#[tokio::test]
async fn aut_023_no_rules_no_queries() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;
    let id = view.issue.id;

    sqlx::query("delete from project_rules where project_id = $1")
        .bind(web)
        .execute(pool)
        .await
        .unwrap();
    let before = issues::get(pool, &admin, id).await.unwrap().0;

    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, web)
        .await
        .unwrap();
    let field = fields.0.first().unwrap().clone();
    let row = taskmaster_store::issues::get(&mut *conn, id)
        .await
        .unwrap()
        .unwrap();
    let mut batch = Batch::new();
    let mut ctx = Ctx::new(&admin);
    let records = rules::on(
        &mut conn,
        &mut batch,
        &mut ctx,
        Event::FieldChanged {
            issue: &row,
            field: &field,
            old: None,
            new: None,
        },
    )
    .await
    .unwrap();
    drop(conn);

    // One read of the toggles, nothing else: no records, no history, the issue untouched.
    assert!(records.is_empty());
    assert!(ctx.messages.is_empty());
    assert_eq!(history(pool, id).await.len(), 1);
    let after = issues::get(pool, &admin, id).await.unwrap().0;
    assert_eq!(after.issue.updated_at, before.issue.updated_at);
}

/// Rule-made changes re-enter the engine boundedly: past either limit the write refuses with an
/// internal error that names the counts (§3.5).
#[tokio::test]
async fn aut_023_depth_bounded() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;

    let mut conn = pool.acquire().await.unwrap();
    let (field, resolved) = state_with_resolved(&mut conn, web).await;
    let row = taskmaster_store::issues::get(&mut *conn, view.issue.id)
        .await
        .unwrap()
        .unwrap();

    let mut ctx = Ctx::new(&admin);
    ctx.depth = MAX_DEPTH;
    let Error::Internal(message) = rules::write(
        &mut conn,
        &mut Batch::new(),
        &mut ctx,
        &row,
        &field,
        Some(&Value::Refs(vec![resolved])),
        Rule::FixedInBuild,
    )
    .await
    .unwrap_err() else {
        panic!("the depth limit refuses the write");
    };
    assert!(message.contains("depth 20"), "{message}");

    let mut ctx = Ctx::new(&admin);
    ctx.writes = MAX_WRITES;
    let Error::Internal(message) = rules::write(
        &mut conn,
        &mut Batch::new(),
        &mut ctx,
        &row,
        &field,
        Some(&Value::Refs(vec![resolved])),
        Rule::FixedInBuild,
    )
    .await
    .unwrap_err() else {
        panic!("the write limit refuses the write");
    };
    assert!(message.contains("100 writes"), "{message}");

    // One step below the limit the write still goes through.
    let mut ctx = Ctx::new(&admin);
    ctx.depth = MAX_DEPTH - 1;
    rules::write(
        &mut conn,
        &mut Batch::new(),
        &mut ctx,
        &row,
        &field,
        Some(&Value::Refs(vec![resolved])),
        Rule::FixedInBuild,
    )
    .await
    .unwrap();
}

/// A rule the project turned off does nothing: no records join, no message rides (§3.4).
#[tokio::test]
async fn aut_023_toggle_off_does_nothing() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;

    let mut conn = pool.acquire().await.unwrap();
    for rule in Rule::ALL {
        taskmaster_store::rules::set_enabled(&mut conn, web, rule, false)
            .await
            .unwrap();
    }
    let (_, resolved) = state_with_resolved(&mut conn, web).await;
    drop(conn);

    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        state_field(&view),
        FieldChange::Set(Some(Input::Integer(resolved))),
        false,
    )
    .await
    .unwrap();
    assert!(changed.messages.is_empty());
    // Only the actor's own records: no rule wrote anything.
    let entries = issue_history::all_of(pool, view.issue.id).await.unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1].origin, "web");
    assert_eq!(entries[2].origin, "web");
    assert_eq!(entries[1].kind, "field");
    assert_eq!(entries[2].kind, "resolved");
}

/// A person with a role in a project, as the other tests' helpers make them.
async fn person(
    pool: &PgPool,
    actor: &Access,
    username: &str,
    role: PredefinedRole,
    of: &[ProjectId],
) -> Access {
    let user = users::create(
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
    .id;
    let role = taskmaster_store::roles::builtin_id(pool, role.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().copied().map(Some).collect();
    taskmaster_service::roles::assign(pool, actor, role, Holder::User(user), &scopes)
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

/// AUT-063: assigning someone the issue's audience does not list is told to the actor; the
/// reporter, a listed account and a member of a listed group are all right, and so is every
/// change on an open issue. Nothing is ever changed or blocked.
#[tokio::test]
async fn aut_063_assignee_not_visible() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let amy = person(pool, &admin, "amy", PredefinedRole::ProjectMember, &[web]).await;
    let bob = person(pool, &admin, "bob", PredefinedRole::ProjectMember, &[web]).await;
    // The assignee must be assignable: the team is the assignee field's set.
    taskmaster_service::teams::add(pool, &admin, web, &[amy.user(), bob.user()], &[])
        .await
        .unwrap();
    let view = create(pool, &admin, web, "Crash on save").await;
    let id = view.issue.id;
    let assignee = view
        .fields
        .iter()
        .find(|f| f.field.field.builtin.as_deref() == Some("assignee"))
        .unwrap()
        .field
        .id();

    // An audience of one account: assigning anyone else is told.
    issues::update(
        pool,
        &admin,
        WEB,
        id,
        IssueChange {
            audience: Some(AudienceInput {
                users: vec![amy.user()],
                groups: vec![],
            }),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        id,
        assignee,
        FieldChange::Set(Some(Input::Integer(bob.user()))),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        changed.messages,
        vec![rules::Message {
            rule: Rule::AssigneeVisibility,
            code: "assignee_not_visible",
            params: BTreeMap::from([("assignee".to_owned(), "bob Person".to_owned())]),
        }]
    );

    // The listed account and the reporter are visible; nothing is said.
    for user in [amy.user(), admin.user()] {
        let changed = issues::set_field(
            pool,
            &admin,
            WEB,
            id,
            assignee,
            FieldChange::Set(Some(Input::Integer(user))),
            false,
        )
        .await
        .unwrap();
        assert!(changed.messages.is_empty(), "{user}");
    }

    // An audience of one group: a member sees the issue, anyone else does not.
    let group = taskmaster_service::groups::create(
        pool,
        &admin,
        taskmaster_service::groups::NewGroup {
            name: "Insiders".to_owned(),
            ..taskmaster_service::groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    taskmaster_service::groups::add_members(pool, &admin, group.id, &[bob.user()])
        .await
        .unwrap();
    issues::update(
        pool,
        &admin,
        WEB,
        id,
        IssueChange {
            audience: Some(AudienceInput {
                users: vec![],
                groups: vec![group.id],
            }),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        id,
        assignee,
        FieldChange::Set(Some(Input::Integer(bob.user()))),
        false,
    )
    .await
    .unwrap();
    assert!(changed.messages.is_empty());
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        id,
        assignee,
        FieldChange::Set(Some(Input::Integer(amy.user()))),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        changed.messages,
        vec![rules::Message {
            rule: Rule::AssigneeVisibility,
            code: "assignee_not_visible",
            params: BTreeMap::from([("assignee".to_owned(), "amy Person".to_owned())]),
        }]
    );

    // An open issue hides nobody; a cleared assignee says nothing.
    issues::update(
        pool,
        &admin,
        WEB,
        id,
        IssueChange {
            audience: Some(AudienceInput::default()),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    for change in [
        FieldChange::Set(Some(Input::Integer(bob.user()))),
        FieldChange::Set(None),
    ] {
        let changed = issues::set_field(pool, &admin, WEB, id, assignee, change, false)
            .await
            .unwrap();
        assert!(changed.messages.is_empty());
    }
}

/// AUT-079: a comment that says `+1` among its words adds the commenter's vote and says so; a
/// second `+1`, the reporter's own and a comment without the word do nothing.
#[tokio::test]
async fn aut_079_plus_one_vote() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let amy = person(pool, &admin, "amy", PredefinedRole::ProjectMember, &[web]).await;
    let view = create(pool, &admin, web, "Crash on save").await;
    let id = view.issue.id;

    fn plus_one(text: &str) -> NewComment {
        NewComment {
            text: text.to_owned(),
            ..NewComment::default()
        }
    }

    // Amy's `+1` becomes her vote, with a word to say about it.
    let answer = comments::create(pool, &amy, WEB, id, plus_one("Yes, +1"))
        .await
        .unwrap();
    assert_eq!(
        answer.messages,
        vec![rules::Message {
            rule: Rule::PlusOneVote,
            code: "vote_added",
            params: BTreeMap::new(),
        }]
    );
    assert_eq!(votes(pool, id).await, 1);

    // A second `+1`, punctuation and all, changes nothing.
    let answer = comments::create(pool, &amy, WEB, id, plus_one("+1, agreed"))
        .await
        .unwrap();
    assert!(answer.messages.is_empty());
    assert_eq!(votes(pool, id).await, 1);

    // The reporter may not vote for their own issue, word or no word.
    let answer = comments::create(pool, &admin, WEB, id, plus_one("+1"))
        .await
        .unwrap();
    assert!(answer.messages.is_empty());
    assert_eq!(votes(pool, id).await, 1);

    // A comment without the word says nothing.
    let answer = comments::create(pool, &amy, WEB, id, plus_one("Looks fixed"))
        .await
        .unwrap();
    assert!(answer.messages.is_empty());
    assert_eq!(votes(pool, id).await, 1);
}

async fn votes(pool: &PgPool, issue: IssueId) -> i32 {
    taskmaster_store::issues::get(pool, issue)
        .await
        .unwrap()
        .unwrap()
        .votes
}

/// The blank project's own set of the auto-attached component field (an empty one).
async fn component_set(conn: &mut sqlx::PgConnection, project: ProjectId) -> (FieldId, i64) {
    let fields = taskmaster_service::issues::values::of_project(conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.field.builtin.as_deref() == Some("component"))
        .unwrap();
    (field.id(), field.attached.set_id.unwrap())
}

/// Adds a value to a set, with an owner or without one, and answers its ID.
async fn owned_value(
    pool: &PgPool,
    actor: &Access,
    set: i64,
    name: &str,
    owner: Option<UserId>,
) -> i64 {
    value_sets::add_value(
        pool,
        actor,
        set,
        ValueInput {
            name: Some(name.to_owned()),
            owner: owner.map(Some),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap()
    .values
    .iter()
    .find(|v| v.name == name)
    .unwrap()
    .id
}

/// AUT-090, FLD-049: an issue that takes a component value with an owner and has nobody assigned
/// gets the owner as its assignee, in the rule's name; an owner outside the assignee field's set
/// is told instead, a value without an owner and an issue that keeps an assignee are left alone.
#[tokio::test]
async fn aut_090_component_owner_assigns() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let bob = person(pool, &admin, "bob", PredefinedRole::ProjectMember, &[web]).await;
    let zed = person(pool, &admin, "zed", PredefinedRole::ProjectMember, &[web]).await;
    // Bob joins the team — the assignee field's set — and Zed does not.
    taskmaster_service::teams::add(pool, &admin, web, &[bob.user()], &[])
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let (component, set) = component_set(&mut conn, web).await;
    let fields = taskmaster_service::issues::values::of_project(&mut conn, web)
        .await
        .unwrap();
    let assignee = fields
        .0
        .iter()
        .find(|f| f.field.builtin.as_deref() == Some("assignee"))
        .unwrap()
        .id();
    drop(conn);
    let server = owned_value(pool, &admin, set, "Server", Some(bob.user())).await;
    let docs = owned_value(pool, &admin, set, "Docs", Some(zed.user())).await;
    let bare = owned_value(pool, &admin, set, "Barebone", None).await;

    // A component with an owner in the set: the owner is assigned, in the rule's name.
    let view = create(pool, &admin, web, "Crash on save").await;
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        component,
        FieldChange::Set(Some(Input::Integer(server))),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        changed.messages,
        vec![rules::Message {
            rule: Rule::ComponentOwner,
            code: "assigned_to_owner",
            params: BTreeMap::from([
                ("assignee".to_owned(), "bob Person".to_owned()),
                ("value".to_owned(), "Server".to_owned()),
            ]),
        }]
    );
    let after = issues::get(pool, &admin, view.issue.id).await.unwrap().0;
    let held = after
        .fields
        .iter()
        .find(|f| f.field.id() == assignee)
        .unwrap();
    assert_eq!(held.value, Some(Value::Refs(vec![bob.user()])));
    let entries = history(pool, view.issue.id).await;
    assert_eq!(entries.len(), 3);
    // The actor's own change, then the rule's write under the same person (§3.12).
    assert_eq!(entries[1], ("web".to_owned(), admin.user()));
    assert_eq!(entries[2], ("rule".to_owned(), admin.user()));

    // An owner outside the assignee field's set is told; nothing changes.
    let view = create(pool, &admin, web, "Docs drift").await;
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        component,
        FieldChange::Set(Some(Input::Integer(docs))),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        changed.messages,
        vec![rules::Message {
            rule: Rule::ComponentOwner,
            code: "owner_not_assignable",
            params: BTreeMap::from([
                ("owner".to_owned(), "zed Person".to_owned()),
                ("value".to_owned(), "Docs".to_owned()),
            ]),
        }]
    );
    let after = issues::get(pool, &admin, view.issue.id).await.unwrap().0;
    let held = after
        .fields
        .iter()
        .find(|f| f.field.id() == assignee)
        .unwrap();
    assert_eq!(held.value, None);

    // A value without an owner, and an issue that keeps an assignee, are left alone.
    let view = create(pool, &admin, web, "Barebone bug").await;
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        component,
        FieldChange::Set(Some(Input::Integer(bare))),
        false,
    )
    .await
    .unwrap();
    assert!(changed.messages.is_empty());
    let view = create(pool, &admin, web, "Assigned bug").await;
    issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        assignee,
        FieldChange::Set(Some(Input::Integer(admin.user()))),
        false,
    )
    .await
    .unwrap();
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        component,
        FieldChange::Set(Some(Input::Integer(server))),
        false,
    )
    .await
    .unwrap();
    assert!(changed.messages.is_empty());
    let after = issues::get(pool, &admin, view.issue.id).await.unwrap().0;
    let held = after
        .fields
        .iter()
        .find(|f| f.field.id() == assignee)
        .unwrap();
    assert_eq!(held.value, Some(Value::Refs(vec![admin.user()])));
}

/// The issue's held value of a field, by the field's built-in key.
async fn held(pool: &PgPool, actor: &Access, issue: IssueId, builtin: &str) -> Option<Value> {
    issues::get(pool, actor, issue)
        .await
        .unwrap()
        .0
        .fields
        .iter()
        .find(|f| f.field.field.builtin.as_deref() == Some(builtin))
        .unwrap()
        .value
        .clone()
}

/// The ID of the built-in link type of subtasks.
async fn subtasks(pool: &PgPool) -> i64 {
    let mut conn = pool.acquire().await.unwrap();
    taskmaster_store::links::type_by_builtin(&mut *conn, "subtask")
        .await
        .unwrap()
        .unwrap()
        .id
}

/// Links a child to its parent through the subtask type, from the child's side (§3.5: the event
/// then names the parent as the other end).
async fn link_parent(pool: &PgPool, actor: &Access, child: IssueId, parent: IssueId) {
    taskmaster_service::issue_links::add(
        pool,
        actor,
        WEB,
        child,
        taskmaster_service::issue_links::NewLinks {
            link_type: subtasks(pool).await,
            side: taskmaster_domain::links::Side::Target,
            issues: vec![parent],
        },
        false,
    )
    .await
    .unwrap();
}

async fn resolve(pool: &PgPool, actor: &Access, view: &IssueView) -> rules::Answer<issues::One> {
    let (_, resolved) = {
        let mut conn = pool.acquire().await.unwrap();
        state_with_resolved(&mut conn, view.issue.project_id).await
    };
    issues::set_field(
        pool,
        actor,
        WEB,
        view.issue.id,
        state_field(view),
        FieldChange::Set(Some(Input::Integer(resolved))),
        false,
    )
    .await
    .unwrap()
}

/// The project's fix-versions field and a version value of its set, by name.
async fn fix_versions(
    pool: &PgPool,
    actor: &Access,
    project: ProjectId,
    names: &[&str],
) -> (FieldId, Vec<i64>) {
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.field.builtin.as_deref() == Some("fix_versions"))
        .unwrap();
    let set = field.attached.set_id.unwrap();
    drop(conn);
    let mut ids = Vec::new();
    for name in names {
        ids.push(owned_value(pool, actor, set, name, None).await);
    }
    (field.id(), ids)
}

/// AUT-094: the last resolved subtask resolves its parent in the rule's name and says so; while a
/// subtask stays open, the parent is left alone.
#[tokio::test]
async fn aut_094_last_subtask_sets_parent() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let parent = create(pool, &admin, web, "Parent").await;
    let one = create(pool, &admin, web, "First").await;
    let two = create(pool, &admin, web, "Second").await;
    link_parent(pool, &admin, one.issue.id, parent.issue.id).await;
    link_parent(pool, &admin, two.issue.id, parent.issue.id).await;

    // One subtask resolved is not all of them: the parent stays open, nothing is said.
    let changed = resolve(pool, &admin, &one).await;
    assert!(changed.messages.is_empty());
    assert!(
        issues::get(pool, &admin, parent.issue.id)
            .await
            .unwrap()
            .0
            .issue
            .resolved_at
            .is_none()
    );

    // The last one resolves the parent, in the rule's name, with a word to the actor.
    let changed = resolve(pool, &admin, &two).await;
    let (_, web_fixed) = {
        let mut conn = pool.acquire().await.unwrap();
        state_with_resolved(&mut conn, web).await
    };
    assert_eq!(
        changed.messages,
        vec![rules::Message {
            rule: Rule::ParentFollowsSubtasks,
            code: "parent_set",
            params: BTreeMap::from([
                (
                    "issue".to_owned(),
                    format!("WEB-{}", parent.issue.number.unwrap())
                ),
                ("value".to_owned(), "Fixed".to_owned()),
            ]),
        }]
    );
    let after = issues::get(pool, &admin, parent.issue.id).await.unwrap().0;
    assert_eq!(
        held(pool, &admin, parent.issue.id, "state").await,
        Some(Value::Refs(vec![web_fixed]))
    );
    assert!(after.issue.resolved_at.is_some());
    // The parent's own change is the rule's, under the triggering actor (§3.12).
    let entries = history(pool, parent.issue.id).await;
    assert!(
        entries
            .iter()
            .any(|(origin, author)| { origin == "rule" && *author == admin.user() })
    );
}

/// AUT-094: the walk goes up while each parent's own subtasks are all resolved — one change of
/// the bottom issue settles the whole chain.
#[tokio::test]
async fn aut_094_recursive_up() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let grand = create(pool, &admin, web, "Grand").await;
    let parent = create(pool, &admin, web, "Parent").await;
    let child = create(pool, &admin, web, "Child").await;
    link_parent(pool, &admin, parent.issue.id, grand.issue.id).await;
    link_parent(pool, &admin, child.issue.id, parent.issue.id).await;

    let changed = resolve(pool, &admin, &child).await;
    assert_eq!(changed.messages.len(), 2);
    assert!(
        changed
            .messages
            .iter()
            .all(|message| message.code == "parent_set")
    );
    for id in [grand.issue.id, parent.issue.id] {
        let view = issues::get(pool, &admin, id).await.unwrap().0;
        assert!(view.issue.resolved_at.is_some(), "{id}");
    }
}

/// AUT-094: a reopened subtask reopens the parents above it, to the value named «Reopened» when
/// the set holds one.
#[tokio::test]
async fn aut_094_reopen_reopens_parents() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let parent = create(pool, &admin, web, "Parent").await;
    let child = create(pool, &admin, web, "Child").await;
    link_parent(pool, &admin, child.issue.id, parent.issue.id).await;
    resolve(pool, &admin, &child).await;
    assert!(
        issues::get(pool, &admin, parent.issue.id)
            .await
            .unwrap()
            .0
            .issue
            .resolved_at
            .is_some()
    );

    // The set's «Reopened» value, by name (the default set holds one).
    let reopened = {
        let mut conn = pool.acquire().await.unwrap();
        let (field, _) = state_with_resolved(&mut conn, web).await;
        let values =
            taskmaster_store::value_sets::values_of(&mut *conn, &[field.attached.set_id.unwrap()])
                .await
                .unwrap();
        values.iter().find(|v| v.name == "Reopened").unwrap().id
    };
    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        child.issue.id,
        state_field(&child),
        FieldChange::Set(Some(Input::Integer(reopened))),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        changed.messages,
        vec![rules::Message {
            rule: Rule::ParentFollowsSubtasks,
            code: "parent_reopened",
            params: BTreeMap::from([(
                "issue".to_owned(),
                format!("WEB-{}", parent.issue.number.unwrap()),
            )]),
        }]
    );
    let after = issues::get(pool, &admin, parent.issue.id).await.unwrap().0;
    assert_eq!(
        held(pool, &admin, parent.issue.id, "state").await,
        Some(Value::Refs(vec![reopened]))
    );
    assert!(after.issue.resolved_at.is_none());
}

/// AUT-094: the walk stops at an archived project (PRJ-062): the parent there is not written and
/// nothing is said.
#[tokio::test]
async fn aut_094_skips_archived() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let old = project(pool, &admin, "OLD").await;
    let parent = create(pool, &admin, old, "Parent").await;
    let child = create(pool, &admin, web, "Child").await;
    link_parent(pool, &admin, child.issue.id, parent.issue.id).await;
    projects::set_archived(pool, &admin, old, true)
        .await
        .unwrap();
    // A new issue holds its state field's default value: the point is that it stays.
    let before = held(pool, &admin, parent.issue.id, "state").await;

    let changed = resolve(pool, &admin, &child).await;
    assert!(changed.messages.is_empty());
    assert_eq!(held(pool, &admin, parent.issue.id, "state").await, before);
    assert!(
        issues::get(pool, &admin, parent.issue.id)
            .await
            .unwrap()
            .0
            .issue
            .resolved_at
            .is_none()
    );
}

/// AUT-094: the value the parent takes is its own project's: a kanban parent's stage set holds
/// «Done», not the shared default set's «Fixed» the child's project resolves to.
#[tokio::test]
async fn aut_094_value_per_project() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let board = projects::create(
        pool,
        &admin,
        NewProjectInput {
            template: Template::Kanban,
            name: "BOARD".to_owned(),
            key: Some("BOARD".to_owned()),
            ..NewProjectInput {
                template: Template::Blank,
                name: String::new(),
                key: None,
                first_number: None,
                description: String::new(),
            }
        },
        Language::En,
    )
    .await
    .unwrap()
    .project
    .id;
    let parent = create(pool, &admin, board, "Parent").await;
    let child = create(pool, &admin, web, "Child").await;
    link_parent(pool, &admin, child.issue.id, parent.issue.id).await;

    let (_, web_fixed) = {
        let mut conn = pool.acquire().await.unwrap();
        state_with_resolved(&mut conn, web).await
    };
    let (_, board_done) = {
        let mut conn = pool.acquire().await.unwrap();
        state_with_resolved(&mut conn, board).await
    };
    assert_ne!(web_fixed, board_done);

    let changed = resolve(pool, &admin, &child).await;
    assert_eq!(changed.messages.len(), 1);
    // The parent's own stage field, by its own set's rank: «Done», not the child's «Fixed».
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, board)
        .await
        .unwrap();
    let stage = fields
        .0
        .iter()
        .find(|f| f.kind.kind == FieldKind::State)
        .unwrap();
    let row = taskmaster_store::issues::get(&mut *conn, parent.issue.id)
        .await
        .unwrap()
        .unwrap();
    let values = taskmaster_service::issues::values::of_issue(&mut conn, &row, &fields)
        .await
        .unwrap();
    drop(conn);
    assert_eq!(
        values.get(&stage.id()),
        Some(&Value::Refs(vec![board_done]))
    );
    assert!(
        issues::get(pool, &admin, parent.issue.id)
            .await
            .unwrap()
            .0
            .issue
            .resolved_at
            .is_some()
    );
}

/// AUT-094: a parent the actor cannot see is still followed — the write has no rights — but
/// nothing is said to the actor about an issue they cannot read.
#[tokio::test]
async fn aut_094_invisible_parent_changed_without_message() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let amy = person(pool, &admin, "amy", PredefinedRole::ProjectMember, &[web]).await;
    let bob = person(pool, &admin, "bob", PredefinedRole::ProjectMember, &[web]).await;
    let parent = create(pool, &amy, web, "Parent").await;
    let child = create(pool, &bob, web, "Child").await;
    link_parent(pool, &bob, child.issue.id, parent.issue.id).await;
    // Amy restricts the parent to herself: Bob, who resolves the child, cannot see it.
    issues::update(
        pool,
        &amy,
        WEB,
        parent.issue.id,
        IssueChange {
            audience: Some(AudienceInput {
                users: vec![amy.user()],
                groups: vec![],
            }),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();

    let changed = resolve(pool, &bob, &child).await;
    assert!(changed.messages.is_empty());
    let (_, fixed) = {
        let mut conn = pool.acquire().await.unwrap();
        state_with_resolved(&mut conn, web).await
    };
    assert_eq!(
        held(pool, &admin, parent.issue.id, "state").await,
        Some(Value::Refs(vec![fixed]))
    );
}

/// AUT-092: a child linked to a parent takes the parent's fix versions by name, in its own
/// project's set, and silently — the records speak.
#[tokio::test]
async fn aut_092_link_copies_from_parent() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let app = project(pool, &admin, "APP").await;
    // Each project has its own empty version set: the same name is another value.
    let (parent_field, parent_versions) = fix_versions(pool, &admin, web, &["1.0", "2.0"]).await;
    let (_, child_versions) = fix_versions(pool, &admin, app, &["1.0", "3.0"]).await;
    let parent = create(pool, &admin, web, "Parent").await;
    let child = create(pool, &admin, app, "Child").await;
    issues::set_field(
        pool,
        &admin,
        WEB,
        parent.issue.id,
        parent_field,
        FieldChange::Set(Some(Input::Ids(parent_versions.clone()))),
        false,
    )
    .await
    .unwrap();

    let answer = taskmaster_service::issue_links::add(
        pool,
        &admin,
        WEB,
        child.issue.id,
        taskmaster_service::issue_links::NewLinks {
            link_type: subtasks(pool).await,
            side: taskmaster_domain::links::Side::Target,
            issues: vec![parent.issue.id],
        },
        false,
    )
    .await
    .unwrap();
    assert!(answer.messages.is_empty());
    // «2.0» is not in the child's set: skipped; «1.0» is the child's own value of that name,
    // not the parent's.
    assert_ne!(parent_versions[0], child_versions[0]);
    assert_eq!(
        held(pool, &admin, child.issue.id, "fix_versions").await,
        Some(Value::Refs(vec![child_versions[0]]))
    );
}

/// AUT-092: a change of the parent's fix versions reaches every open subtask.
#[tokio::test]
async fn aut_092_parent_change_updates_subtasks() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let (parent_field, versions) = fix_versions(pool, &admin, web, &["1.0", "2.0"]).await;
    let parent = create(pool, &admin, web, "Parent").await;
    let child = create(pool, &admin, web, "Child").await;
    link_parent(pool, &admin, child.issue.id, parent.issue.id).await;

    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        parent.issue.id,
        parent_field,
        FieldChange::Set(Some(Input::Ids(versions.clone()))),
        false,
    )
    .await
    .unwrap();
    assert!(changed.messages.is_empty());
    assert_eq!(
        held(pool, &admin, child.issue.id, "fix_versions").await,
        Some(Value::Refs(versions.clone()))
    );

    // A resolved subtask keeps what it holds.
    let other = create(pool, &admin, web, "Done already").await;
    link_parent(pool, &admin, other.issue.id, parent.issue.id).await;
    resolve(pool, &admin, &other).await;
    issues::set_field(
        pool,
        &admin,
        WEB,
        parent.issue.id,
        parent_field,
        FieldChange::Set(Some(Input::Ids(vec![versions[0]]))),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        held(pool, &admin, other.issue.id, "fix_versions").await,
        Some(Value::Refs(versions.clone()))
    );
}

/// AUT-092: names the child's set does not hold are skipped; when none match, a field that may be
/// empty is cleared.
#[tokio::test]
async fn aut_092_missing_names_skipped() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let app = project(pool, &admin, "APP").await;
    let (parent_field, parent_versions) = fix_versions(pool, &admin, web, &["9.9"]).await;
    let (_, child_versions) = fix_versions(pool, &admin, app, &["1.0"]).await;
    let parent = create(pool, &admin, web, "Parent").await;
    let child = create(pool, &admin, app, "Child").await;
    issues::set_field(
        pool,
        &admin,
        WEB,
        child.issue.id,
        parent_field,
        FieldChange::Set(Some(Input::Ids(child_versions.clone()))),
        false,
    )
    .await
    .unwrap();
    link_parent(pool, &admin, child.issue.id, parent.issue.id).await;
    issues::set_field(
        pool,
        &admin,
        WEB,
        parent.issue.id,
        parent_field,
        FieldChange::Set(Some(Input::Ids(parent_versions))),
        false,
    )
    .await
    .unwrap();
    // The child's set holds neither «9.9»: nothing matches, and the field may be empty.
    assert_eq!(
        held(pool, &admin, child.issue.id, "fix_versions").await,
        None
    );
}

/// AUT-092: a subtask whose project has no fix-versions field is skipped, without a word.
#[tokio::test]
async fn aut_092_other_projects_field_absent() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let bare = project(pool, &admin, "BARE").await;
    let (parent_field, versions) = fix_versions(pool, &admin, web, &["1.0"]).await;
    // The other project has no fix-versions field at all.
    let mut conn = pool.acquire().await.unwrap();
    let bare_field = taskmaster_service::issues::values::of_project(&mut conn, bare)
        .await
        .unwrap()
        .0
        .iter()
        .find(|f| f.field.builtin.as_deref() == Some("fix_versions"))
        .unwrap()
        .id();
    drop(conn);
    taskmaster_service::project_fields::detach(pool, &admin, bare, bare_field)
        .await
        .unwrap();
    let parent = create(pool, &admin, web, "Parent").await;
    let child = create(pool, &admin, bare, "Child").await;
    link_parent(pool, &admin, child.issue.id, parent.issue.id).await;

    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        parent.issue.id,
        parent_field,
        FieldChange::Set(Some(Input::Ids(versions))),
        false,
    )
    .await
    .unwrap();
    assert!(changed.messages.is_empty());
    let view = issues::get(pool, &admin, child.issue.id).await.unwrap().0;
    assert!(
        view.fields
            .iter()
            .all(|f| f.field.field.builtin.as_deref() != Some("fix_versions"))
    );
}

/// PRJ-062: a rule never writes into an archived project — `write` itself refuses, whatever the
/// behavior asked; no records, no history, no message.
#[tokio::test]
async fn prj_062_archived_rules_silent() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;
    projects::set_archived(pool, &admin, web, true)
        .await
        .unwrap();

    let mut conn = pool.acquire().await.unwrap();
    let (field, resolved) = state_with_resolved(&mut conn, web).await;
    let row = taskmaster_store::issues::get(&mut *conn, view.issue.id)
        .await
        .unwrap()
        .unwrap();
    let before = history(pool, view.issue.id).await.len();
    let held_before = held_of_archived(pool, view.issue.id).await;
    let mut batch = Batch::new();
    let mut ctx = Ctx::new(&admin);
    let records = rules::write(
        &mut conn,
        &mut batch,
        &mut ctx,
        &row,
        &field,
        Some(&Value::Refs(vec![resolved])),
        Rule::ParentFollowsSubtasks,
    )
    .await
    .unwrap();
    drop(conn);
    assert!(records.is_empty());
    assert!(ctx.messages.is_empty());
    assert_eq!(history(pool, view.issue.id).await.len(), before);
    assert_eq!(held_of_archived(pool, view.issue.id).await, held_before);
}

/// The state of an issue of an archived project, read by hand: the field was not written.
async fn held_of_archived(pool: &PgPool, issue: IssueId) -> Option<Value> {
    let mut conn = pool.acquire().await.unwrap();
    let row = taskmaster_store::issues::get(&mut *conn, issue)
        .await
        .unwrap()
        .unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, row.project_id)
        .await
        .unwrap();
    let values = taskmaster_service::issues::values::of_issue(&mut conn, &row, &fields)
        .await
        .unwrap();
    let state = fields
        .0
        .iter()
        .find(|f| f.kind.kind == FieldKind::State)
        .unwrap();
    values.get(&state.id()).cloned()
}

/// The ID of the built-in duplicate link type.
async fn duplicates(pool: &PgPool) -> i64 {
    let mut conn = pool.acquire().await.unwrap();
    taskmaster_store::links::type_by_builtin(&mut *conn, "duplicate")
        .await
        .unwrap()
        .unwrap()
        .id
}

/// Links a duplicate to its original from the duplicate's side: the original is the source of the
/// link (§3.8), the way a child links its parent.
async fn link_duplicate(
    pool: &PgPool,
    actor: &Access,
    duplicate: IssueId,
    original: IssueId,
) -> rules::Answer<taskmaster_service::issue_links::Added> {
    taskmaster_service::issue_links::add(
        pool,
        actor,
        WEB,
        duplicate,
        taskmaster_service::issue_links::NewLinks {
            link_type: duplicates(pool).await,
            side: taskmaster_domain::links::Side::Target,
            issues: vec![original],
        },
        false,
    )
    .await
    .unwrap()
}

/// The project's build field and build values of its set, by name.
async fn builds(
    pool: &PgPool,
    actor: &Access,
    project: ProjectId,
    names: &[&str],
) -> (FieldId, Vec<i64>) {
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.field.builtin.as_deref() == Some("fixed_in_build"))
        .unwrap();
    let set = field.attached.set_id.unwrap();
    drop(conn);
    let mut ids = Vec::new();
    for name in names {
        ids.push(owned_value(pool, actor, set, name, None).await);
    }
    (field.id(), ids)
}

/// The project's state field, its first open value and its first resolved one.
async fn state_pair(pool: &PgPool, project: ProjectId) -> (FieldId, i64, i64) {
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.kind.kind == FieldKind::State)
        .unwrap();
    let set = field.attached.set_id.unwrap();
    let values = taskmaster_store::value_sets::values_of(&mut *conn, &[set])
        .await
        .unwrap();
    let open = values.iter().find(|v| !v.resolved).unwrap();
    let resolved = values.iter().find(|v| v.resolved).unwrap();
    (field.id(), open.id, resolved.id)
}

/// The state set's «Дубликат» value (§3.8): the shared default set holds it already.
async fn duplicate_state(pool: &PgPool, project: ProjectId) -> i64 {
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.kind.kind == FieldKind::State)
        .unwrap();
    let set = field.attached.set_id.unwrap();
    let values = taskmaster_store::value_sets::values_of(&mut *conn, &[set])
        .await
        .unwrap();
    values.iter().find(|v| v.name == "Duplicate").unwrap().id
}

/// Sets one field of an issue to a set value.
async fn set_value(
    pool: &PgPool,
    actor: &Access,
    issue: IssueId,
    field: FieldId,
    value: Option<i64>,
) -> rules::Answer<issues::One> {
    issues::set_field(
        pool,
        actor,
        WEB,
        issue,
        field,
        FieldChange::Set(value.map(Input::Integer)),
        false,
    )
    .await
    .unwrap()
}

/// AUT-098: a reopened issue loses the build it was fixed in; resolving keeps it, and the clear
/// says nothing to the actor.
#[tokio::test]
async fn aut_098_unresolved_clears_build() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let (build, builds) = builds(pool, &admin, web, &["2026.11"]).await;
    let (state, open, resolved) = state_pair(pool, web).await;
    let issue = create(pool, &admin, web, "Crash on save").await;
    set_value(pool, &admin, issue.issue.id, build, Some(builds[0])).await;

    // Resolving keeps the build: only a reopen clears it.
    set_value(pool, &admin, issue.issue.id, state, Some(resolved)).await;
    assert_eq!(
        held(pool, &admin, issue.issue.id, "fixed_in_build").await,
        Some(Value::Refs(vec![builds[0]]))
    );

    let reopened = set_value(pool, &admin, issue.issue.id, state, Some(open)).await;
    assert!(reopened.messages.is_empty());
    assert_eq!(
        held(pool, &admin, issue.issue.id, "fixed_in_build").await,
        None
    );
}

/// AUT-098: an issue whose state names it a duplicate takes the build of its original; an
/// original without a build gives nothing. Silent — the records speak.
#[tokio::test]
async fn aut_098_duplicate_state_copies_build() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let (build, builds) = builds(pool, &admin, web, &["2026.10", "2026.11"]).await;
    let (state, _, _) = state_pair(pool, web).await;
    let duplicate = duplicate_state(pool, web).await;

    let original = create(pool, &admin, web, "Original").await;
    set_value(pool, &admin, original.issue.id, build, Some(builds[1])).await;
    let dup = create(pool, &admin, web, "Same crash").await;
    link_duplicate(pool, &admin, dup.issue.id, original.issue.id).await;
    let changed = set_value(pool, &admin, dup.issue.id, state, Some(duplicate)).await;
    assert!(changed.messages.is_empty());
    assert_eq!(
        held(pool, &admin, dup.issue.id, "fixed_in_build").await,
        Some(Value::Refs(vec![builds[1]]))
    );

    // An original with no build gives nothing.
    let bare = create(pool, &admin, web, "Bare report").await;
    let dup2 = create(pool, &admin, web, "Its duplicate").await;
    link_duplicate(pool, &admin, dup2.issue.id, bare.issue.id).await;
    set_value(pool, &admin, dup2.issue.id, state, Some(duplicate)).await;
    assert_eq!(
        held(pool, &admin, dup2.issue.id, "fixed_in_build").await,
        None
    );
}

/// AUT-098: a build value set by hand spreads to the issue's duplicates; clearing the field does
/// not clear theirs.
#[tokio::test]
async fn aut_098_build_spreads_to_duplicates() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let (build, builds) = builds(pool, &admin, web, &["2026.11"]).await;
    let original = create(pool, &admin, web, "Original").await;
    let first = create(pool, &admin, web, "First duplicate").await;
    let second = create(pool, &admin, web, "Second duplicate").await;
    link_duplicate(pool, &admin, first.issue.id, original.issue.id).await;
    link_duplicate(pool, &admin, second.issue.id, original.issue.id).await;

    let changed = set_value(pool, &admin, original.issue.id, build, Some(builds[0])).await;
    assert!(changed.messages.is_empty());
    assert_eq!(
        held(pool, &admin, first.issue.id, "fixed_in_build").await,
        Some(Value::Refs(vec![builds[0]]))
    );
    assert_eq!(
        held(pool, &admin, second.issue.id, "fixed_in_build").await,
        Some(Value::Refs(vec![builds[0]]))
    );

    // A cleared build does not clear the duplicates'.
    set_value(pool, &admin, original.issue.id, build, None).await;
    assert_eq!(
        held(pool, &admin, first.issue.id, "fixed_in_build").await,
        Some(Value::Refs(vec![builds[0]]))
    );
}

/// AUT-098: duplicates of another project never take the build — neither by their state nor by
/// the original's change.
#[tokio::test]
async fn aut_098_cross_project_untouched() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let app = project(pool, &admin, "APP").await;
    let (build, builds) = builds(pool, &admin, web, &["2026.11"]).await;
    let (app_state, _, _) = state_pair(pool, app).await;
    let duplicate = duplicate_state(pool, app).await;
    let original = create(pool, &admin, web, "Original").await;
    let foreign = create(pool, &admin, app, "Foreign duplicate").await;
    link_duplicate(pool, &admin, foreign.issue.id, original.issue.id).await;

    // The foreign duplicate's state copies nothing from another project's original.
    set_value(pool, &admin, foreign.issue.id, app_state, Some(duplicate)).await;
    assert_eq!(
        held(pool, &admin, foreign.issue.id, "fixed_in_build").await,
        None
    );

    // The original's build does not cross the project line either.
    set_value(pool, &admin, original.issue.id, build, Some(builds[0])).await;
    assert_eq!(
        held(pool, &admin, foreign.issue.id, "fixed_in_build").await,
        None
    );
}

// ---------------------------------------------------------------------------------------------
// AUT-071: duplicates (part 1.9b-1 — rules 1–3 of §3.8)

/// The issues an issue is a duplicate of: the sources of its duplicate links (§3.8).
async fn originals(pool: &PgPool, issue: IssueId) -> Vec<IssueId> {
    let mut conn = pool.acquire().await.unwrap();
    let type_id = duplicates(pool).await;
    let mut ids: Vec<IssueId> = taskmaster_store::links::sources_of(&mut *conn, type_id, &[issue])
        .await
        .unwrap()
        .into_iter()
        .map(|(_, source)| source)
        .collect();
    ids.sort_unstable();
    ids
}

/// The project's priority field and the ID of its set's value with a name.
async fn priority_value(pool: &PgPool, project: ProjectId, name: &str) -> (FieldId, i64) {
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.field.builtin.as_deref() == Some("priority"))
        .unwrap();
    let set = field.attached.set_id.unwrap();
    let value = taskmaster_store::value_sets::values_of(&mut *conn, &[set])
        .await
        .unwrap()
        .iter()
        .find(|v| v.name == name)
        .unwrap()
        .id;
    (field.id(), value)
}

/// The project's state set value with a name (the default set holds «Reopened», §3.7).
async fn state_value_named(pool: &PgPool, project: ProjectId, name: &str) -> i64 {
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.kind.kind == FieldKind::State)
        .unwrap();
    let set = field.attached.set_id.unwrap();
    taskmaster_store::value_sets::values_of(&mut *conn, &[set])
        .await
        .unwrap()
        .iter()
        .find(|v| v.name == name)
        .unwrap()
        .id
}

/// AUT-071 rule 1: a duplicate of an issue that is a duplicate itself hangs directly under the
/// chain's root; the actor hears where the link landed.
#[tokio::test]
async fn aut_071_chain_collapsed_to_root() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let root = create(pool, &admin, web, "The root").await;
    let middle = create(pool, &admin, web, "A duplicate already").await;
    let fresh = create(pool, &admin, web, "A new duplicate").await;
    link_duplicate(pool, &admin, middle.issue.id, root.issue.id).await;

    let answer = link_duplicate(pool, &admin, fresh.issue.id, middle.issue.id).await;
    // The new link went to the root; the middle stays a duplicate of the root.
    assert_eq!(originals(pool, fresh.issue.id).await, vec![root.issue.id]);
    assert_eq!(originals(pool, middle.issue.id).await, vec![root.issue.id]);
    assert_eq!(answer.messages.len(), 1, "{:?}", answer.messages);
    assert_eq!(answer.messages[0].rule, Rule::Duplicates);
    assert_eq!(answer.messages[0].code, "link_reattached");
    assert_eq!(answer.messages[0].params["issue"], "WEB-1");
    // The rewrite is a rule's: the record carries the rule origin (§3.12).
    assert!(
        history(pool, fresh.issue.id)
            .await
            .iter()
            .any(|(origin, _)| origin == "rule")
    );
}

/// AUT-071 rule 1: an issue that becomes a duplicate hands its own duplicates to the same root —
/// the group stays a star, one root under every duplicate.
#[tokio::test]
async fn aut_071_children_reattached() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let original = create(pool, &admin, web, "The original").await;
    let turning = create(pool, &admin, web, "Becomes a duplicate").await;
    let first = create(pool, &admin, web, "Its first duplicate").await;
    let second = create(pool, &admin, web, "Its second duplicate").await;
    link_duplicate(pool, &admin, first.issue.id, turning.issue.id).await;
    link_duplicate(pool, &admin, second.issue.id, turning.issue.id).await;

    let answer = link_duplicate(pool, &admin, turning.issue.id, original.issue.id).await;
    assert!(answer.messages.is_empty(), "{:?}", answer.messages);
    // The duplicates it anchored now hang under the original, as it does.
    assert_eq!(
        originals(pool, turning.issue.id).await,
        vec![original.issue.id]
    );
    assert_eq!(
        originals(pool, first.issue.id).await,
        vec![original.issue.id]
    );
    assert_eq!(
        originals(pool, second.issue.id).await,
        vec![original.issue.id]
    );
    // Every moved link left a record in the rule's name (§3.12).
    for issue in [turning.issue.id, first.issue.id, second.issue.id] {
        assert!(
            history(pool, issue)
                .await
                .iter()
                .any(|(origin, _)| origin == "rule")
        );
    }
}

/// AUT-071 rule 2: the original takes its new duplicate's priority when that comes earlier in
/// the one set both fields share — and the actor hears the raise.
#[tokio::test]
async fn aut_071_priority_raised_same_set() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let original = create(pool, &admin, web, "The original").await;
    let loud = create(pool, &admin, web, "The louder duplicate").await;
    let (priority, high) = priority_value(pool, web, "High").await;
    let (_, normal) = priority_value(pool, web, "Normal").await;
    set_value(pool, &admin, original.issue.id, priority, Some(normal)).await;
    set_value(pool, &admin, loud.issue.id, priority, Some(high)).await;

    let answer = link_duplicate(pool, &admin, loud.issue.id, original.issue.id).await;
    assert_eq!(
        held(pool, &admin, original.issue.id, "priority").await,
        Some(Value::Refs(vec![high]))
    );
    assert_eq!(answer.messages.len(), 1, "{:?}", answer.messages);
    assert_eq!(answer.messages[0].code, "priority_raised");
    assert_eq!(answer.messages[0].params["value"], "High");
    // The duplicate keeps its own priority.
    assert_eq!(
        held(pool, &admin, loud.issue.id, "priority").await,
        Some(Value::Refs(vec![high]))
    );
}

/// AUT-071 rule 2: projects whose priority fields answer to different sets never compare — a
/// forked set is its own order (§3.8).
#[tokio::test]
async fn aut_071_priority_other_set_ignored() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let app = project(pool, &admin, "APP").await;
    // APP forks its priority set: the same order, another set.
    let (app_priority, _) = priority_value(pool, app, "High").await;
    taskmaster_service::project_fields::fork_set(pool, &admin, app, app_priority)
        .await
        .unwrap();
    let original = create(pool, &admin, app, "The original").await;
    let loud = create(pool, &admin, web, "The louder duplicate").await;
    let (priority, high) = priority_value(pool, web, "High").await;
    let (_, app_normal) = priority_value(pool, app, "Normal").await;
    set_value(pool, &admin, loud.issue.id, priority, Some(high)).await;
    set_value(
        pool,
        &admin,
        original.issue.id,
        app_priority,
        Some(app_normal),
    )
    .await;

    let answer = link_duplicate(pool, &admin, loud.issue.id, original.issue.id).await;
    assert!(answer.messages.is_empty(), "{:?}", answer.messages);
    assert_eq!(
        held(pool, &admin, original.issue.id, "priority").await,
        Some(Value::Refs(vec![app_normal]))
    );
}

/// AUT-071 rule 3: an issue whose last duplicate-of link is gone and whose state still names it
/// a duplicate takes the reopen value — silently, the records speak.
#[tokio::test]
async fn aut_071_reopen_when_links_removed() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let (state, _, _) = state_pair(pool, web).await;
    let duplicate = duplicate_state(pool, web).await;
    let reopened = state_value_named(pool, web, "Reopened").await;
    let original = create(pool, &admin, web, "The original").await;
    let gone = create(pool, &admin, web, "The duplicate").await;
    link_duplicate(pool, &admin, gone.issue.id, original.issue.id).await;
    set_value(pool, &admin, gone.issue.id, state, Some(duplicate)).await;
    let mut conn = pool.acquire().await.unwrap();
    let type_id = duplicates(pool).await;
    let link = taskmaster_store::links::of_issue(&mut *conn, gone.issue.id)
        .await
        .unwrap()
        .into_iter()
        .find(|end| end.type_id == type_id)
        .unwrap()
        .link;
    drop(conn);

    let messages =
        taskmaster_service::issue_links::remove(pool, &admin, WEB, gone.issue.id, link, false)
            .await
            .unwrap();
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(
        held(pool, &admin, gone.issue.id, "state").await,
        Some(Value::Refs(vec![reopened]))
    );
    assert!(
        history(pool, gone.issue.id)
            .await
            .iter()
            .any(|(origin, _)| origin == "rule")
    );
}

// ---------------------------------------------------------------------------------------------
// AUT-071: duplicates (part 1.9b-2 — rules 4–5 of §3.8)

/// A person whose role in a project holds exactly the named permissions.
async fn limited(
    pool: &PgPool,
    actor: &Access,
    username: &str,
    permissions: &[&str],
    project: ProjectId,
) -> Access {
    let user = users::create(
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
    .id;
    let role = roles::create(
        pool,
        actor,
        roles::NewRole {
            name: format!("Role of {username}"),
            description: String::new(),
            permissions: permissions.iter().map(|p| (*p).to_owned()).collect(),
        },
    )
    .await
    .unwrap()
    .role
    .id;
    roles::assign(pool, actor, role, Holder::User(user), &[Some(project)])
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

/// AUT-071 rule 4: an issue made a duplicate takes the duplicate value — the write resolves it
/// (`settle_resolution` runs inside) and its records carry the rule's origin (§3.12).
#[tokio::test]
async fn aut_071_state_set_on_link() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let original = create(pool, &admin, web, "The original").await;
    let fresh = create(pool, &admin, web, "The duplicate").await;
    let duplicate = duplicate_state(pool, web).await;

    link_duplicate(pool, &admin, fresh.issue.id, original.issue.id).await;
    assert_eq!(
        held(pool, &admin, fresh.issue.id, "state").await,
        Some(Value::Refs(vec![duplicate]))
    );
    assert!(
        history(pool, fresh.issue.id)
            .await
            .iter()
            .any(|(origin, _)| origin == "rule")
    );
    let mut conn = pool.acquire().await.unwrap();
    let row = taskmaster_store::issues::get(&mut *conn, fresh.issue.id)
        .await
        .unwrap()
        .unwrap();
    drop(conn);
    assert!(row.resolved_at.is_some());
}

/// AUT-071 rule 5: the duplicate state demands its link — the value without an original is
/// refused where the form shows field problems, so nothing lands; with an original the same
/// change goes through.
#[tokio::test]
async fn aut_071_state_requires_link() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let issue = create(pool, &admin, web, "No link yet").await;
    let (state, open, _) = state_pair(pool, web).await;
    let duplicate = duplicate_state(pool, web).await;

    // Another value of the same field is none of the rule's business.
    set_value(pool, &admin, issue.issue.id, state, Some(open)).await;
    let refused = issues::set_field(
        pool,
        &admin,
        WEB,
        issue.issue.id,
        state,
        FieldChange::Set(Some(Input::Integer(duplicate))),
        false,
    )
    .await
    .unwrap_err();
    let Error::Invalid(problems) = &refused else {
        panic!("a missing link is a field problem, not {refused:?}")
    };
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].path, "value");
    assert_eq!(problems[0].problem, Problem::Required);
    assert_eq!(problems[0].message, "add a link to the duplicate issue");
    assert_eq!(
        held(pool, &admin, issue.issue.id, "state").await,
        Some(Value::Refs(vec![open]))
    );

    // With an original the same change goes through.
    let original = create(pool, &admin, web, "The original").await;
    link_duplicate(pool, &admin, issue.issue.id, original.issue.id).await;
    set_value(pool, &admin, issue.issue.id, state, Some(open)).await;
    set_value(pool, &admin, issue.issue.id, state, Some(duplicate)).await;
    assert_eq!(
        held(pool, &admin, issue.issue.id, "state").await,
        Some(Value::Refs(vec![duplicate]))
    );
}

/// AUT-071 rule 4: the state follows the link only when the actor may write the issue — a linker
/// without `issue.update` leaves the state alone (the reference's guard).
#[tokio::test]
async fn aut_071_rule4_respects_actor_rights() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let original = create(pool, &admin, web, "The original").await;
    let fresh = create(pool, &admin, web, "The duplicate").await;
    let duplicate = duplicate_state(pool, web).await;
    let linker = limited(pool, &admin, "linker", &["issue.read", "issue.link"], web).await;

    link_duplicate(pool, &linker, fresh.issue.id, original.issue.id).await;
    assert_ne!(
        held(pool, &admin, fresh.issue.id, "state").await,
        Some(Value::Refs(vec![duplicate]))
    );
    assert!(
        !history(pool, fresh.issue.id)
            .await
            .iter()
            .any(|(origin, _)| origin == "rule")
    );
}

/// AUT-071 and AUT-098 together: the state change that names a duplicate copies the original's
/// build in the same transaction — the rule's own write on the link, and the actor's by hand.
#[tokio::test]
async fn aut_071_and_098_build_copied_on_duplicate_state() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let (build, builds) = builds(pool, &admin, web, &["2026.11"]).await;
    let (state, open, _) = state_pair(pool, web).await;
    let duplicate = duplicate_state(pool, web).await;
    let original = create(pool, &admin, web, "The original").await;
    let fresh = create(pool, &admin, web, "The duplicate").await;
    set_value(pool, &admin, original.issue.id, build, Some(builds[0])).await;

    // Rule 4's write of the duplicate state copies the original's build in the link's own
    // transaction (§3.8 with §3.7).
    link_duplicate(pool, &admin, fresh.issue.id, original.issue.id).await;
    assert_eq!(
        held(pool, &admin, fresh.issue.id, "state").await,
        Some(Value::Refs(vec![duplicate]))
    );
    assert_eq!(
        held(pool, &admin, fresh.issue.id, "fixed_in_build").await,
        Some(Value::Refs(vec![builds[0]]))
    );

    // The same change by hand rides the same interplay: reopened the build goes, duplicate
    // again — the link is there, rule 5 lets it through — and the build comes back.
    set_value(pool, &admin, fresh.issue.id, state, Some(open)).await;
    assert_eq!(
        held(pool, &admin, fresh.issue.id, "fixed_in_build").await,
        None
    );
    set_value(pool, &admin, fresh.issue.id, state, Some(duplicate)).await;
    assert_eq!(
        held(pool, &admin, fresh.issue.id, "fixed_in_build").await,
        Some(Value::Refs(vec![builds[0]]))
    );
}

// AUT-085: the scrum rules (part 1.9b-3 — §3.9)

/// A scrum project: its template turns the five rules of PRJ-023 on, `scrum` among them, and
/// attaches the fields the rule needs — the state set with Done, the type set with Epic and Task,
/// story points and ideal days.
async fn scrum_project(pool: &PgPool, actor: &Access, key: &str) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Scrum,
            name: key.to_owned(),
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

/// The attached field by its built-in key, with a value of its set by English name.
async fn scrum_value(
    pool: &PgPool,
    project: ProjectId,
    builtin: &str,
    name: &str,
) -> (FieldId, i64) {
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, project)
        .await
        .unwrap();
    let field = fields
        .0
        .iter()
        .find(|f| f.field.builtin.as_deref() == Some(builtin))
        .unwrap()
        .clone();
    let values =
        taskmaster_store::value_sets::values_of(&mut *conn, &[field.attached.set_id.unwrap()])
            .await
            .unwrap();
    let value = values.iter().find(|v| v.name == name).unwrap();
    (field.id(), value.id)
}

/// The estimation field of a scrum project by English name (it has no built-in key).
async fn estimation_field(pool: &PgPool, project: ProjectId, name: &str) -> FieldId {
    let mut conn = pool.acquire().await.unwrap();
    let fields = taskmaster_service::issues::values::of_project(&mut conn, project)
        .await
        .unwrap();
    fields
        .0
        .iter()
        .find(|f| f.kind.kind == FieldKind::Integer && f.field.name == name)
        .unwrap()
        .id()
}

/// AUT-085: a state change on a published, unassigned issue gives it to the actor — silently, in
/// the rule's name; an issue that keeps an assignee is left alone.
#[tokio::test]
async fn aut_085_assign_on_state_change() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = scrum_project(pool, &admin, "SCR").await;
    let (state, progress) = scrum_value(pool, web, "state", "In progress").await;
    let fresh = create(pool, &admin, web, "Unassigned story").await;

    let changed = issues::set_field(
        pool,
        &admin,
        WEB,
        fresh.issue.id,
        state,
        FieldChange::Set(Some(Input::Integer(progress))),
        false,
    )
    .await
    .unwrap();
    assert!(
        changed.messages.is_empty(),
        "the record says it, not a toast"
    );
    assert_eq!(
        held(pool, &admin, fresh.issue.id, "assignee").await,
        Some(Value::Refs(vec![admin.user()]))
    );
    let entries = history(pool, fresh.issue.id).await;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1], ("web".to_owned(), admin.user()));
    assert_eq!(entries[2], ("rule".to_owned(), admin.user()));

    // An issue that keeps an assignee is left alone.
    let mine = create(pool, &admin, web, "Someone else's story").await;
    let assignee = mine
        .fields
        .iter()
        .find(|f| f.field.field.builtin.as_deref() == Some("assignee"))
        .unwrap()
        .field
        .id();
    let bob = person(pool, &admin, "bob", PredefinedRole::ProjectMember, &[web]).await;
    taskmaster_service::teams::add(pool, &admin, web, &[bob.user()], &[])
        .await
        .unwrap();
    set_value(pool, &admin, mine.issue.id, assignee, Some(bob.user())).await;
    set_value(pool, &admin, mine.issue.id, state, Some(progress)).await;
    assert_eq!(
        held(pool, &admin, mine.issue.id, "assignee").await,
        Some(Value::Refs(vec![bob.user()]))
    );
}

/// AUT-085: a board card move is a state change the same way — the card an unassigned issue's
/// lands in a column takes the mover as its assignee.
#[tokio::test]
async fn aut_085_board_move_assigns() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = scrum_project(pool, &admin, "SCR").await;
    let story = create(pool, &admin, web, "Board story").await;
    let listed = taskmaster_service::boards::list(pool, &admin, None, Some(web), None, 10)
        .await
        .unwrap();
    let board = taskmaster_service::boards::get(pool, &admin, listed.items[0].id)
        .await
        .unwrap();
    let sprint = board.full.sprints.iter().find(|s| !s.internal).unwrap().id;
    taskmaster_service::boards::cards::add(
        pool,
        &admin,
        WEB,
        board.full.board.id,
        sprint,
        taskmaster_service::boards::cards::NewCard {
            issue: story.issue.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        held(pool, &admin, story.issue.id, "assignee").await,
        None,
        "joining the board changes no state"
    );

    // A move to another column is the state change; the mover takes the issue.
    let progress = board.full.columns[1].id;
    let version = issues::get(pool, &admin, story.issue.id)
        .await
        .unwrap()
        .0
        .issue
        .version;
    let moved = taskmaster_service::boards::cards::move_card(
        pool,
        &admin,
        WEB,
        board.full.board.id,
        sprint,
        story.issue.id,
        taskmaster_service::boards::cards::Move {
            lane: None,
            column: progress,
            after: None,
            before: None,
            version,
            header: None,
        },
    )
    .await
    .unwrap();
    assert!(moved.messages.is_empty());
    assert_eq!(
        held(pool, &admin, story.issue.id, "assignee").await,
        Some(Value::Refs(vec![admin.user()]))
    );
    assert!(
        history(pool, story.issue.id)
            .await
            .iter()
            .any(|(origin, _)| origin == "rule")
    );
}

/// AUT-085: an actor outside the assignee field's set moves the card too, but takes nothing —
/// the issue stays unassigned (FLD-008, the way AUT-090 leaves an owner outside the set).
#[tokio::test]
async fn aut_085_actor_outside_set_skipped() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = scrum_project(pool, &admin, "SCR").await;
    let outsider = limited(
        pool,
        &admin,
        "outsider",
        &["issue.read", "issue.update"],
        web,
    )
    .await;
    let story = create(pool, &admin, web, "Not their story").await;
    let (state, progress) = scrum_value(pool, web, "state", "In progress").await;

    issues::set_field(
        pool,
        &outsider,
        WEB,
        story.issue.id,
        state,
        FieldChange::Set(Some(Input::Integer(progress))),
        false,
    )
    .await
    .unwrap();
    assert_eq!(held(pool, &admin, story.issue.id, "assignee").await, None);
    assert!(
        !history(pool, story.issue.id)
            .await
            .iter()
            .any(|(origin, _)| origin == "rule")
    );
}

/// AUT-085: an epic counts no ideal days — the change that would set them is refused where the
/// form shows field problems, clearing is allowed, and a task's days are none of the rule's
/// business.
#[tokio::test]
async fn aut_085_epic_days_blocked() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = scrum_project(pool, &admin, "SCR").await;
    let (kind, epic) = scrum_value(pool, web, "type", "Epic").await;
    let (_, task) = scrum_value(pool, web, "type", "Task").await;
    let days = estimation_field(pool, web, "Ideal days").await;
    let view = create(pool, &admin, web, "The epic").await;
    set_value(pool, &admin, view.issue.id, kind, Some(epic)).await;

    let refused = issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        days,
        FieldChange::Set(Some(Input::Integer(3))),
        false,
    )
    .await
    .unwrap_err();
    let Error::Invalid(problems) = &refused else {
        panic!("an epic's days are a field problem, not {refused:?}")
    };
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].path, "value");
    assert_eq!(problems[0].problem, Problem::Invalid);
    assert_eq!(problems[0].message, "epics do not count ideal days");

    // Clearing is allowed; a task counts its days.
    set_value(pool, &admin, view.issue.id, days, None).await;
    set_value(pool, &admin, view.issue.id, kind, Some(task)).await;
    set_value(pool, &admin, view.issue.id, days, Some(3)).await;
}

/// AUT-085: a task counts no story points — refused where the form shows field problems, while
/// an epic's points are none of the rule's business.
#[tokio::test]
async fn aut_085_task_points_blocked() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = scrum_project(pool, &admin, "SCR").await;
    let (kind, epic) = scrum_value(pool, web, "type", "Epic").await;
    let (_, task) = scrum_value(pool, web, "type", "Task").await;
    let points = estimation_field(pool, web, "Story points").await;
    let view = create(pool, &admin, web, "The task").await;
    set_value(pool, &admin, view.issue.id, kind, Some(task)).await;

    let refused = issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        points,
        FieldChange::Set(Some(Input::Integer(5))),
        false,
    )
    .await
    .unwrap_err();
    let Error::Invalid(problems) = &refused else {
        panic!("a task's points are a field problem, not {refused:?}")
    };
    assert_eq!(problems[0].problem, Problem::Invalid);
    assert_eq!(problems[0].message, "tasks do not count story points");

    set_value(pool, &admin, view.issue.id, kind, Some(epic)).await;
    set_value(pool, &admin, view.issue.id, points, Some(5)).await;
}

/// AUT-085: the parent-follows walk under the `scrum` toggle — a scrum project without
/// `subtasks_follow` resolves its parents all the way up, to the set's first resolved value
/// («Готово»/Done of the scrum set), and says so.
#[tokio::test]
async fn aut_085_parent_done_recursively() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = scrum_project(pool, &admin, "SCR").await;
    let grandparent = create(pool, &admin, web, "Grandparent").await;
    let parent = create(pool, &admin, web, "Parent").await;
    let child = create(pool, &admin, web, "Child").await;
    link_parent(pool, &admin, parent.issue.id, grandparent.issue.id).await;
    link_parent(pool, &admin, child.issue.id, parent.issue.id).await;

    let changed = resolve(pool, &admin, &child).await;
    let (_, done) = scrum_value(pool, web, "state", "Done").await;
    // One step of the walk speaks per parent, both in the scrum rule's name.
    assert_eq!(changed.messages.len(), 2);
    assert!(
        changed
            .messages
            .iter()
            .all(|message| message.rule == Rule::Scrum && message.code == "parent_set")
    );
    assert_eq!(
        changed.messages[0].params.get("value").map(String::as_str),
        Some("Done")
    );
    assert_eq!(
        held(pool, &admin, parent.issue.id, "state").await,
        Some(Value::Refs(vec![done]))
    );
    // The climb goes on by itself: the parent's resolution resolves the grandparent.
    assert!(
        issues::get(pool, &admin, grandparent.issue.id)
            .await
            .unwrap()
            .0
            .issue
            .resolved_at
            .is_some()
    );
    assert_eq!(
        held(pool, &admin, grandparent.issue.id, "state").await,
        Some(Value::Refs(vec![done]))
    );
}

// ---------------------------------------------------------------------------------------------
// AUT-066: cloning (part 1.9b-4 — §3.10)

/// A published copy at once: the suffixed summary, a creation record naming the original, and
/// the message that names the copy for the toast.
#[tokio::test]
async fn aut_066_clone_published() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;

    let done = rules::clone::clone_issue(pool, &admin, WEB, view.issue.id)
        .await
        .unwrap();
    assert_eq!(done.done.0.issue.number, Some(2));
    assert_eq!(done.done.0.issue.project_id, web);
    assert_eq!(done.done.0.issue.summary, "Crash on save (copy)");
    assert_eq!(done.done.0.readable_id().as_deref(), Some("WEB-2"));
    assert_eq!(
        done.messages,
        vec![rules::Message {
            rule: Rule::CloneIssue,
            code: "clone_created",
            params: BTreeMap::from([("issue".to_owned(), "WEB-2".to_owned())]),
        }]
    );
    // The creation record names the original; the copy starts with no other history.
    let entries = issue_history::all_of(pool, done.done.0.issue.id)
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, "created");
    assert_eq!(entries[0].origin, "web");
    assert_eq!(entries[0].author_id, admin.user());
    assert_eq!(
        entries[0].added,
        Some(serde_json::json!({ "clone_of": { "id": view.issue.id } }))
    );
}

/// The same copies as the actor's drafts: no number, no history, one message per source, and
/// the source itself untouched.
#[tokio::test]
async fn aut_066_clone_as_draft() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;

    let done = rules::clone::clone_drafts(pool, &admin, &[view.issue.id, view.issue.id])
        .await
        .unwrap();
    // A repeated ask makes one copy.
    assert_eq!(done.done.issues.len(), 1);
    let draft = &done.done.issues[0];
    assert!(draft.issue.number.is_none());
    assert_eq!(draft.issue.summary, "Crash on save (copy)");
    assert_eq!(draft.issue.reporter_id, admin.user());
    assert_eq!(
        done.messages,
        vec![rules::Message {
            rule: Rule::CloneIssue,
            code: "draft_saved",
            params: BTreeMap::from([("issue".to_owned(), "WEB-1".to_owned())]),
        }]
    );
    // A draft has no history; the source keeps its number.
    assert!(
        issue_history::all_of(pool, draft.issue.id)
            .await
            .unwrap()
            .is_empty()
    );
    let source = issues::get(pool, &admin, view.issue.id).await.unwrap().0;
    assert_eq!(source.issue.number, Some(1));
}

/// The copy takes every field value, the original's audience and the actor's own tags — and
/// none of other people's tags, votes or watchers.
#[tokio::test]
async fn aut_066_copies_values_tags_audience() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let amy = person(pool, &admin, "amy", PredefinedRole::ProjectMember, &[web]).await;
    let view = create(pool, &admin, web, "Crash on save").await;
    let mut conn = pool.acquire().await.unwrap();
    let (field, resolved) = state_with_resolved(&mut conn, web).await;
    drop(conn);
    issues::set_field(
        pool,
        &admin,
        WEB,
        view.issue.id,
        field.id(),
        FieldChange::Set(Some(Input::Integer(resolved))),
        false,
    )
    .await
    .unwrap();
    issues::update(
        pool,
        &admin,
        WEB,
        view.issue.id,
        IssueChange {
            audience: Some(AudienceInput {
                users: vec![amy.user()],
                groups: vec![],
            }),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    let mine = taskmaster_service::tags::create(pool, &admin, "mine", None)
        .await
        .unwrap();
    let theirs = taskmaster_service::tags::create(pool, &amy, "theirs", None)
        .await
        .unwrap();
    taskmaster_service::tags::set_on_issue(pool, &admin, WEB, view.issue.id, mine.id, true)
        .await
        .unwrap();
    taskmaster_service::tags::set_on_issue(pool, &amy, WEB, view.issue.id, theirs.id, true)
        .await
        .unwrap();
    issues::set_vote(pool, &amy, view.issue.id, true)
        .await
        .unwrap();

    let done = rules::clone::clone_issue(pool, &admin, WEB, view.issue.id)
        .await
        .unwrap();
    let clone = issues::get(pool, &admin, done.done.0.issue.id)
        .await
        .unwrap()
        .0;
    // Every field value: the resolved state, with the resolution time of the copy.
    assert_eq!(
        held(pool, &admin, clone.issue.id, "state").await,
        Some(Value::Refs(vec![resolved]))
    );
    assert!(clone.issue.resolved_at.is_some());
    // The original's audience.
    assert_eq!(clone.issue.audience_users, vec![amy.user()]);
    assert!(clone.issue.audience_groups.is_empty());
    // The actor's own tags only.
    let tags: Vec<_> = clone.tags.iter().map(|t| t.id).collect();
    assert_eq!(tags, vec![mine.id]);
    // Votes stay with the original.
    assert_eq!(clone.issue.votes, 0);
}

/// Both use cases need `issue.create` besides read of the source; without read of the source
/// there is nothing to find.
#[tokio::test]
async fn aut_066_needs_create_right() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;

    let reader = limited(pool, &admin, "reader", &["issue.read"], web).await;
    assert!(matches!(
        rules::clone::clone_issue(pool, &reader, WEB, view.issue.id).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        rules::clone::clone_drafts(pool, &reader, &[view.issue.id]).await,
        Err(Error::Forbidden(_))
    ));
    let blind = limited(pool, &admin, "blind", &["project.basic"], web).await;
    assert!(matches!(
        rules::clone::clone_issue(pool, &blind, WEB, view.issue.id).await,
        Err(Error::NotFound(_))
    ));
}

/// A project that turned `clone_issue` off refuses both use cases and copies nothing.
#[tokio::test]
async fn aut_066_toggle_off() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "WEB").await;
    let view = create(pool, &admin, web, "Crash on save").await;
    let mut conn = pool.acquire().await.unwrap();
    taskmaster_store::rules::set_enabled(&mut conn, web, Rule::CloneIssue, false)
        .await
        .unwrap();
    drop(conn);

    assert!(matches!(
        rules::clone::clone_issue(pool, &admin, WEB, view.issue.id).await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        rules::clone::clone_drafts(pool, &admin, &[view.issue.id]).await,
        Err(Error::Refused(_))
    ));
    assert!(
        issues::drafts(pool, &admin)
            .await
            .unwrap()
            .issues
            .is_empty()
    );
}
