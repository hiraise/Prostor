//! The command engine (step 2.1a-2, plan §4): bare values, field kinds, add/remove/replace on
//! multi-value fields, the create-value quick fix, assignee shortcuts, the comment command,
//! atomicity — one transaction per issue — and permissions, on a real database.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;
use taskmaster_domain::BoardId;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::boards::Preset;
use taskmaster_domain::issues::history::Origin;
use taskmaster_domain::projects::Template;
use taskmaster_domain::settings::Language;
use taskmaster_domain::{IssueId, ProjectId, UserId};
use taskmaster_query::command::CommandCode;
use taskmaster_service::boards::sprints::{self as sprints_service, NewSprint};
use taskmaster_service::boards::{self as boards_service, NewBoard, Sharing};
use taskmaster_service::commands::bulk;
use taskmaster_service::commands::{self, ApplyInput, Operation};
use taskmaster_service::issues::{self, NewIssue};
use taskmaster_service::projects::{self, NewProjectInput};
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::teams;
use taskmaster_service::users::{self, NewAccount};
use taskmaster_service::{Error, Passwords, access, roles, value_sets};
use taskmaster_store::PgPool;
use taskmaster_store::access::Holder;
use taskmaster_store::jobs::{self as jobs_queue, Handlers, JobState, Runner, RunnerConfig};
use taskmaster_store::sessions::Origin as SessionOrigin;
use taskmaster_testkit::TestDb;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const WEB: Origin = Origin::Web;

fn passwords() -> Passwords {
    Passwords::new(4)
}

async fn admin(pool: &PgPool) -> Access {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    let signed_in = setup::run(
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
    .unwrap();
    access::load(pool, signed_in.user.id).await.unwrap()
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

/// A project member: the daily-work role of a project.
async fn member(pool: &PgPool, admin: &Access, username: &str, of: ProjectId) -> Access {
    let user = users::create(
        pool,
        &passwords(),
        admin,
        NewAccount {
            username: username.to_owned(),
            full_name: format!("{username} Person"),
            email: format!("{username}@example.com", username = username),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let role = taskmaster_store::roles::builtin_id(pool, PredefinedRole::ProjectMember.key())
        .await
        .unwrap()
        .unwrap();
    roles::assign(pool, admin, role, Holder::User(user), &[Some(of)])
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

async fn issue(pool: &PgPool, actor: &Access, project: ProjectId) -> IssueId {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: "Commanded".to_owned(),
            publish: true,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id
}

/// A draft of the actor's own (ISS-006): only its reporter's, and it takes no comments.
async fn draft(pool: &PgPool, actor: &Access, project: ProjectId) -> IssueId {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: "Draft".to_owned(),
            publish: false,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id
}

/// Adds a value to the set of the project's field, as the setup of a test. The set is read off
/// one throwaway issue of the project.
async fn value_of(pool: &PgPool, actor: &Access, project: ProjectId, field: &str, name: &str) {
    let probe = issue(pool, actor, project).await;
    let view = issues::get(pool, actor, probe).await.unwrap();
    let set = view
        .0
        .fields
        .iter()
        .find(|f| f.field.field.name == field)
        .unwrap()
        .field
        .attached
        .set_id
        .unwrap();
    value_sets::add_value(
        pool,
        actor,
        set,
        value_sets::ValueInput {
            name: Some(name.to_owned()),
            ..value_sets::ValueInput::default()
        },
        None,
    )
    .await
    .unwrap();
}

/// The names of the issue's values of one field, by field name, sorted: set values by their
/// titles, accounts by their full names.
async fn names_of(pool: &PgPool, actor: &Access, issue: IssueId, field: &str) -> Vec<String> {
    use taskmaster_domain::fields::FieldKind;
    let view = issues::get(pool, actor, issue).await.unwrap();
    let Some(found) = view.0.fields.iter().find(|f| f.field.field.name == field) else {
        return Vec::new();
    };
    let Some(value) = &found.value else {
        return Vec::new();
    };
    let mut conn = pool.acquire().await.unwrap();
    if found.field.kind.kind == FieldKind::User {
        let users = taskmaster_store::users::get_many(&mut *conn, value.refs())
            .await
            .unwrap();
        users.iter().map(|u| u.full_name.clone()).collect()
    } else {
        let mut rows = taskmaster_store::value_sets::get_values(&mut *conn, value.refs())
            .await
            .unwrap();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        rows.iter().map(|v| v.name.clone()).collect()
    }
}

async fn comment_count(pool: &PgPool, issue: IssueId) -> i64 {
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from comments where issue_id = $1"#,
        issue
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

fn input(text: &str, issues: &[IssueId]) -> ApplyInput {
    ApplyInput {
        text: text.to_owned(),
        issues: issues.to_vec(),
        ..ApplyInput::default()
    }
}

fn codes(answer: &commands::ApplyAnswer) -> Vec<CommandCode> {
    answer.plan.problems.iter().map(|p| p.code).collect()
}

/// cmd_003: bare values resolve in the fixed order (plan §4.2): a tag name tags, a username
/// assigns, a field value sets, a project name moves (2.1a-3 applies the move), and a project
/// colliding with a tag is ambiguous — never a guess.
#[tokio::test]
async fn cmd_003_bare_values_in_the_fixed_order() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let mars = project(pool, &admin, "Mars", "MARS").await;
    let bob = member(pool, &admin, "Bob", mars).await;
    teams::add(pool, &admin, mars, &[bob.user()], &[])
        .await
        .unwrap();
    value_of(pool, &admin, mars, "Fix versions", "2025.3").await;
    let target = issue(pool, &admin, mars).await;

    // A bare tag name tags the issue.
    let tag = taskmaster_service::tags::create(pool, &bob, "web", None)
        .await
        .unwrap();
    let answer = commands::apply(pool, &bob, WEB, &input("web", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok);
    let view = issues::get(pool, &bob, target).await.unwrap();
    assert!(view.0.tags.iter().any(|t| t.id == tag.id));

    // A bare username assigns the one user field.
    let answer = commands::apply(pool, &admin, WEB, &input("bob", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert_eq!(
        names_of(pool, &admin, target, "Assignee").await,
        ["Bob Person".to_owned()]
    );

    // A bare value fitting several fields names them; the user qualifies (plan §4.2).
    let answer = commands::apply(pool, &bob, WEB, &input("2025.3", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdAmbiguous]);
    assert_eq!(
        answer.plan.problems[0]
            .params
            .get("fields")
            .map(String::as_str),
        Some("Fix versions, Affected versions")
    );
    let answer = commands::apply(pool, &bob, WEB, &input("fix versions 2025.3", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(
        names_of(pool, &bob, target, "Fix versions").await,
        ["2025.3".to_owned()]
    );

    // A bare project name resolves to a move (applied from 2.1a-3).
    let answer = commands::apply(pool, &admin, WEB, &input("mars", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(
        answer
            .plan
            .operations
            .iter()
            .any(|planned| matches!(planned.operation, Operation::Move { .. }))
    );

    // A project name and a tag name colliding is ambiguous.
    taskmaster_service::tags::create(pool, &bob, "Mars", None)
        .await
        .unwrap();
    let answer = commands::apply(pool, &bob, WEB, &input("mars", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdAmbiguous]);

    // A lone word nothing matches is the parser's unknown word; a name after a command word
    // that matches nothing is the resolution's unknown value (plan §3, §4.2).
    let answer = commands::apply(pool, &bob, WEB, &input("nosuchthing", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdUnknownWord]);
    let answer = commands::apply(pool, &bob, WEB, &input("tag nosuchtag", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdUnknownValue]);
}

/// cmd_004: each field kind takes its own shape (plan §4.2): set values by name, dates absolute
/// and relative, durations in work units; clearing what may not be empty is refused, computed
/// fields are refused.
#[tokio::test]
async fn cmd_004_field_kinds() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    // A date field: `Due date` is not attached to new projects (FLD-020).
    taskmaster_service::project_fields::attach(
        pool,
        &admin,
        web,
        taskmaster_service::project_fields::AttachInput::New {
            name: "Deadline".to_owned(),
            kind: taskmaster_domain::fields::FieldKind::Date,
            multi: false,
            aliases: Vec::new(),
        },
    )
    .await
    .unwrap();
    let target = issue(pool, &admin, web).await;

    // A state value by name, multi-word.
    let answer = commands::apply(pool, &bob, WEB, &input("state In progress", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(
        names_of(pool, &bob, target, "State").await,
        ["In progress".to_owned()]
    );

    // An absolute date.
    let answer = commands::apply(pool, &bob, WEB, &input("deadline 2026-11-20", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    let view = issues::get(pool, &bob, target).await.unwrap();
    let due = view
        .0
        .fields
        .iter()
        .find(|f| f.field.field.name == "Deadline")
        .unwrap();
    assert_eq!(
        due.value.as_ref().map(|v| format!("{:?}", v)),
        Some("Date(2026-11-20)".to_owned())
    );

    // A relative date resolves to a concrete one in the caller's zone; a name that is not a
    // date is refused.
    let planned = commands::plan(
        pool,
        &bob,
        &ApplyInput {
            text: "deadline today".to_owned(),
            issues: vec![target],
            time_zone: Some("Europe/Moscow".to_owned()),
            ..ApplyInput::default()
        },
    )
    .await
    .unwrap();
    assert!(planned.problems.is_empty(), "{:?}", planned.problems);
    let answer = commands::apply(pool, &bob, WEB, &input("deadline next week", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    let answer = commands::apply(pool, &bob, WEB, &input("deadline critical", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdWrongKind]);

    // A duration takes its work units; a word does not parse.
    let answer = commands::apply(pool, &bob, WEB, &input("estimation 2h", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    let answer = commands::apply(pool, &bob, WEB, &input("estimation lots", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdWrongKind]);

    // Clearing a field that may not be empty is refused.
    let answer = commands::apply(pool, &bob, WEB, &input("state none", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdNotEmpty]);

    // The empty name clears where the field may be empty.
    let answer = commands::apply(pool, &bob, WEB, &input("assignee Unassigned", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(names_of(pool, &bob, target, "Assignee").await.is_empty());

    // A computed field is refused.
    let answer = commands::apply(pool, &bob, WEB, &input("spent time 10", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdWrongKind]);
}

/// cmd_006: a plain set replaces a multi-value field's whole list (owner, 2026-11-13); `add`
/// and `remove` keep the other values.
#[tokio::test]
async fn cmd_006_add_remove_and_replace() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    for name in ["2025.1", "2025.2", "2025.3"] {
        value_of(pool, &admin, web, "Fix versions", name).await;
    }
    let target = issue(pool, &admin, web).await;

    // Start from two values.
    commands::apply(
        pool,
        &bob,
        WEB,
        &input("add fix versions 2025.1", &[target]),
    )
    .await
    .unwrap();
    commands::apply(
        pool,
        &bob,
        WEB,
        &input("add fix versions 2025.2", &[target]),
    )
    .await
    .unwrap();
    assert_eq!(
        names_of(pool, &bob, target, "Fix versions").await,
        ["2025.1".to_owned(), "2025.2".to_owned()]
    );

    // A plain set leaves only the named value.
    let answer = commands::apply(pool, &bob, WEB, &input("fix versions 2025.3", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(
        names_of(pool, &bob, target, "Fix versions").await,
        ["2025.3".to_owned()]
    );

    // `add` keeps the others; `remove` takes one out.
    commands::apply(
        pool,
        &bob,
        WEB,
        &input("add fix versions 2025.1", &[target]),
    )
    .await
    .unwrap();
    assert_eq!(
        names_of(pool, &bob, target, "Fix versions").await,
        ["2025.1".to_owned(), "2025.3".to_owned()]
    );
    let answer = commands::apply(
        pool,
        &bob,
        WEB,
        &input("remove fix versions 2025.3", &[target]),
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(
        names_of(pool, &bob, target, "Fix versions").await,
        ["2025.1".to_owned()]
    );

    // `add` on a single-value field is refused before anything runs.
    let answer = commands::apply(pool, &bob, WEB, &input("add state Fixed", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdWrongKind]);
}

/// cmd_007: a value the field lacks is a problem with the create-value quick fix; applying
/// with `create_missing` makes the value and the plan resolves again (plan §4.2, §4.4).
#[tokio::test]
async fn cmd_007_quick_fix_creates_the_missing_value() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(pool, &bob, WEB, &input("fix versions 2026.1", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdUnknownValue]);
    let fix = answer.plan.problems[0].quick_fix.clone().unwrap();
    assert_eq!(fix.name, "2026.1");
    assert_eq!(fix.projects, [web]);
    assert!(!answer.issues[0].ok);

    // Without the permission to update the set, the fix stays a problem.
    let answer = commands::apply(
        pool,
        &bob,
        WEB,
        &ApplyInput {
            create_missing: true,
            ..input("fix versions 2026.1", &[target])
        },
    )
    .await
    .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdUnknownValue]);

    // With it, the value is created and the command applies.
    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &ApplyInput {
            create_missing: true,
            ..input("fix versions 2026.1", &[target])
        },
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok);
    assert_eq!(
        names_of(pool, &bob, target, "Fix versions").await,
        ["2026.1".to_owned()]
    );
}

/// cmd_008: assignee shortcuts — a username, «me», the empty name; two accounts sharing a full
/// name are ambiguous (plan §4.2).
#[tokio::test]
async fn cmd_008_assignee_shortcuts() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    teams::add(pool, &admin, web, &[bob.user()], &[])
        .await
        .unwrap();
    let target = issue(pool, &admin, web).await;

    // By field name and by bare username.
    let answer = commands::apply(pool, &admin, WEB, &input("assignee Bob Person", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(
        names_of(pool, &admin, target, "Assignee").await,
        ["Bob Person".to_owned()]
    );

    // «me» names the actor.
    let answer = commands::apply(pool, &bob, WEB, &input("assignee me", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(
        names_of(pool, &bob, target, "Assignee").await,
        ["Bob Person".to_owned()]
    );

    // The alias works too.
    let answer = commands::apply(pool, &admin, WEB, &input("assigned to bob", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(
        names_of(pool, &admin, target, "Assignee").await,
        ["Bob Person".to_owned()]
    );

    // Two accounts sharing a full name are ambiguous.
    users::create(
        pool,
        &passwords(),
        &admin,
        NewAccount {
            username: "bob2".to_owned(),
            full_name: "Bob Person".to_owned(),
            email: "bob2@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap();
    let answer = commands::apply(pool, &admin, WEB, &input("assignee Bob Person", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdAmbiguous]);
}

/// cmd_019: the `comment` command attaches the dialog's comment; without text it is a problem
/// over the word (plan §4.3, §6).
#[tokio::test]
async fn cmd_019_comment_command() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    let target = issue(pool, &admin, web).await;

    // The word without text is refused.
    let answer = commands::apply(pool, &bob, WEB, &input("comment", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdValueExpected]);
    assert_eq!(comment_count(pool, target).await, 0);

    // With text, the comment posts and joins the same change.
    let answer = commands::apply(
        pool,
        &bob,
        WEB,
        &ApplyInput {
            comment: Some("Done by command.".to_owned()),
            ..input("state Fixed comment", &[target])
        },
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok);
    assert_eq!(comment_count(pool, target).await, 1);
    assert_eq!(
        names_of(pool, &bob, target, "State").await,
        ["Fixed".to_owned()]
    );
}

/// cmd_023: one transaction per issue — a failure inside an issue rolls back everything the
/// line had already done to it, and other issues still stand on their own (plan §4.3).
#[tokio::test]
async fn cmd_023_one_transaction_per_issue() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    // A draft of bob's own: it takes no comments, so the line fails on its last operation,
    // after the field writes of the same transaction.
    let one = draft(pool, &bob, web).await;
    let two = issue(pool, &admin, web).await;

    let answer = commands::apply(
        pool,
        &bob,
        WEB,
        &ApplyInput {
            comment: Some("rolls back".to_owned()),
            ..input("state Fixed priority Low comment", &[one, two])
        },
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(!answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert_eq!(
        answer.issues[0].problem.as_ref().unwrap().code,
        CommandCode::CmdWrongKind
    );
    assert!(answer.issues[1].ok, "{:?}", answer.issues[1].problem);
    // The failed issue is exactly as it was: the defaults, and no comment.
    assert_eq!(names_of(pool, &bob, one, "State").await, ["New".to_owned()]);
    assert_eq!(
        names_of(pool, &bob, one, "Priority").await,
        ["Normal".to_owned()]
    );
    assert_eq!(comment_count(pool, one).await, 0);
    // The other issue took the whole line.
    assert_eq!(
        names_of(pool, &admin, two, "State").await,
        ["Fixed".to_owned()]
    );
    assert_eq!(
        names_of(pool, &admin, two, "Priority").await,
        ["Low".to_owned()]
    );
    assert_eq!(comment_count(pool, two).await, 1);

    // A target that is gone answers `cmd_target_not_found` and touches nothing.
    let gone = issue(pool, &admin, web).await;
    issues::delete(pool, &admin, WEB, gone).await.unwrap();
    let answer = commands::apply(pool, &admin, WEB, &input("state Fixed", &[gone]))
        .await
        .unwrap();
    assert!(!answer.issues[0].ok);
    assert_eq!(
        answer.issues[0].problem.as_ref().unwrap().code,
        CommandCode::CmdTargetNotFound
    );
}

/// cmd_024: permissions (plan §4.4) — knowable before applying, a plan problem; depending on
/// the issue, a per-issue result.
#[tokio::test]
async fn cmd_024_permissions() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let target = issue(pool, &admin, web).await;

    // An account with no role in the project: the plan names the permission and nothing runs.
    let user = users::create(
        pool,
        &passwords(),
        &admin,
        NewAccount {
            username: "watcher".to_owned(),
            full_name: "Wanda Watcher".to_owned(),
            email: "watcher@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let watcher = access::load(pool, user).await.unwrap();
    let answer = commands::apply(pool, &watcher, WEB, &input("state Fixed", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdNoPermission]);
    assert_eq!(
        answer.plan.problems[0]
            .params
            .get("permission")
            .map(String::as_str),
        Some("issue.update")
    );
    assert_eq!(
        names_of(pool, &admin, target, "State").await,
        ["New".to_owned()]
    );

    // The comment permission is knowable the same way; a member has it.
    let answer = commands::apply(
        pool,
        &watcher,
        WEB,
        &ApplyInput {
            comment: Some("no".to_owned()),
            ..input("comment", &[target])
        },
    )
    .await
    .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdNoPermission]);
    let member = member(pool, &admin, "cam", web).await;
    let answer = commands::apply(
        pool,
        &member,
        WEB,
        &ApplyInput {
            comment: Some("yes".to_owned()),
            ..input("comment", &[target])
        },
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok);

    // Too many issues is refused before any read (§4.7's explicit bound).
    let many: Vec<IssueId> = (0..51).map(|_| target).collect();
    let error = commands::apply(pool, &member, WEB, &input("state Fixed", &many))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
}

/// The plan answers problems over ranges and applies nothing (plan §4.2): the dialog keeps its
/// apply off while any error stands (CMD-028's service half).
#[tokio::test]
async fn cmd_023_errors_block_the_apply() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(pool, &admin, WEB, &input("xyz state Fixed", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdUnknownWord]);
    assert!(!answer.issues[0].ok);
    assert_eq!(
        names_of(pool, &admin, target, "State").await,
        ["New".to_owned()]
    );
}

/// The readable ID of an issue (`WEB-2`), for the commands that name issues.
async fn readable(pool: &PgPool, actor: &Access, issue: IssueId) -> String {
    issues::get(pool, actor, issue)
        .await
        .unwrap()
        .0
        .readable_id()
        .unwrap()
}

/// cmd_009: `tag`/`untag` name a tag the actor may use (CMD-009, plan §4.2).
#[tokio::test]
async fn cmd_009_tag_commands() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let target = issue(pool, &admin, web).await;
    taskmaster_service::tags::create(pool, &admin, "urgent", None)
        .await
        .unwrap();

    let answer = commands::apply(pool, &admin, WEB, &input("tag urgent", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok);
    let mut conn = pool.acquire().await.unwrap();
    let tagged = taskmaster_store::tags::on_issues(&mut *conn, &[target], admin.user(), &[])
        .await
        .unwrap();
    drop(conn);
    assert_eq!(tagged.len(), 1);

    let answer = commands::apply(pool, &admin, WEB, &input("untag urgent", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok);
    let mut conn = pool.acquire().await.unwrap();
    let tagged = taskmaster_store::tags::on_issues(&mut *conn, &[target], admin.user(), &[])
        .await
        .unwrap();
    drop(conn);
    assert_eq!(tagged.len(), 0);

    // A tag that is not is a problem over its name.
    let answer = commands::apply(pool, &admin, WEB, &input("tag nope", &[target]))
        .await
        .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdUnknownValue]);
}

/// cmd_010: star for oneself needs only reading; another watcher needs
/// `issue.watchers.update` (CMD-010, plan §4.4).
#[tokio::test]
async fn cmd_010_star_and_watchers() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "bob", web).await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(pool, &bob, WEB, &input("star", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert_eq!(
        taskmaster_store::issues::watched(pool, &[target], bob.user())
            .await
            .unwrap(),
        [target]
    );

    // `watcher` names another account: the daily-work role may manage watchers.
    let answer = commands::apply(pool, &admin, WEB, &input("watcher bob", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert_eq!(
        taskmaster_store::issues::watched(pool, &[target], bob.user())
            .await
            .unwrap(),
        [target]
    );
    // An account with no role in the project cannot even read the issue: per-issue refusal.
    let eve = member(
        pool,
        &admin,
        "eve",
        project(pool, &admin, "Elsewhere", "ELSE").await,
    )
    .await;
    let answer = commands::apply(pool, &eve, WEB, &input("watcher bob", &[target]))
        .await
        .unwrap();
    assert!(
        answer.issues[0]
            .problem
            .as_ref()
            .is_some_and(|problem| problem.code == CommandCode::CmdTargetNotFound)
    );

    let answer = commands::apply(pool, &bob, WEB, &input("unstar", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok);
    assert!(
        taskmaster_store::issues::watched(pool, &[target], bob.user())
            .await
            .unwrap()
            .is_empty()
    );
}

/// cmd_011: votes — anyone who reads, never one's own issue (`cmd_own_issue_vote`, CMD-011).
#[tokio::test]
async fn cmd_011_votes() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "bob", web).await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(pool, &bob, WEB, &input("+1", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let counted: i64 = i64::from(
        sqlx::query_scalar!("select votes from issues where id = $1", target)
            .fetch_one(pool)
            .await
            .unwrap(),
    );
    assert_eq!(counted, 1);

    let answer = commands::apply(pool, &admin, WEB, &input("vote", &[target]))
        .await
        .unwrap();
    assert_eq!(
        answer.issues[0].problem.as_ref().map(|p| p.code),
        Some(CommandCode::CmdOwnIssueVote)
    );

    let answer = commands::apply(pool, &bob, WEB, &input("unvote", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok);
    let counted: i64 = i64::from(
        sqlx::query_scalar!("select votes from issues where id = $1", target)
            .fetch_one(pool)
            .await
            .unwrap(),
    );
    assert_eq!(counted, 0);
}

/// cmd_012: links by word and readable ID — including an old ID after a move — and removal
/// (CMD-012, plan §4.5).
#[tokio::test]
async fn cmd_012_link_commands() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let one = issue(pool, &admin, web).await;
    let two = issue(pool, &admin, web).await;
    let two_id = readable(pool, &admin, two).await;

    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &input(&format!("related to {two_id}"), &[one]),
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok);
    let ends = taskmaster_store::links::of_issue(pool, one).await.unwrap();
    assert!(ends.iter().any(|end| end.other.id == two));

    // The other direction unlinks.
    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &input(&format!("remove related to {two_id}"), &[one]),
    )
    .await
    .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let ends = taskmaster_store::links::of_issue(pool, one).await.unwrap();
    assert!(ends.is_empty());
}

/// cmd_013: `duplicate of` is the duplicate command — the «Дубликат» state and the built-in
/// link in one (CMD-013, plan §4.2); the built-in handling of AUT-071 runs.
#[tokio::test]
async fn cmd_013_duplicate_in_one_step() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let one = issue(pool, &admin, web).await;
    let two = issue(pool, &admin, web).await;
    let two_id = readable(pool, &admin, two).await;

    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &input(&format!("duplicate of {two_id}"), &[one]),
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert_eq!(
        names_of(pool, &admin, one, "State").await,
        ["Duplicate".to_owned()]
    );
    let ends = taskmaster_store::links::of_issue(pool, one).await.unwrap();
    assert!(ends.iter().any(|end| end.other.id == two));
}

/// cmd_014: `move to` moves — the number and project change, the old ID still leads to the
/// issue (CMD-014, plan §4.5).
#[tokio::test]
async fn cmd_014_move_command() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let mars = project(pool, &admin, "Mars", "MARS").await;
    let target = issue(pool, &admin, web).await;
    let old_id = readable(pool, &admin, target).await;

    let answer = commands::apply(pool, &admin, WEB, &input("move to Mars", &[target]))
        .await
        .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let view = issues::get(pool, &admin, target).await.unwrap();
    assert_eq!(view.0.project.key, "MARS");
    let (found, current) = issues::resolve(pool, &admin, &old_id).await.unwrap();
    assert_eq!(found, target);
    assert!(!current);
    assert_eq!(view.0.project.id, mars);
}

/// cmd_015: visibility — a plain `visible to` replaces the audience, `add`/`remove` keep the
/// rest (CMD-015, plan §1, §4.5).
#[tokio::test]
async fn cmd_015_visibility_commands() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "bob", web).await;
    let cam = member(pool, &admin, "cam", web).await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(pool, &admin, WEB, &input("visible to bob", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let row = sqlx::query!(
        "select audience_users, audience_groups from issues where id = $1",
        target
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(row.audience_users, vec![bob.user()]);
    assert!(row.audience_groups.is_empty());

    // `add` keeps bob; a plain set would have replaced the list.
    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &input("add visible to Cam Person", &[target]),
    )
    .await
    .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let row = sqlx::query!("select audience_users from issues where id = $1", target)
        .fetch_one(pool)
        .await
        .unwrap();
    let mut both = row.audience_users.clone();
    both.sort_unstable();
    assert_eq!(both, vec![bob.user(), cam.user()]);

    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &input("remove visible to bob", &[target]),
    )
    .await
    .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let row = sqlx::query!("select audience_users from issues where id = $1", target)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(row.audience_users, vec![cam.user()]);
}

/// cmd_016: `board <name> <sprint>` adds to the named sprint, `remove board <name>` leaves
/// every sprint of the board (CMD-016, plan §4.5).
#[tokio::test]
async fn cmd_016_board_commands() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let target = issue(pool, &admin, web).await;
    let board = boards_service::create(
        pool,
        &admin,
        Language::En,
        NewBoard {
            preset: Preset::Scrum,
            name: "Board One".to_owned(),
            projects: vec![web],
            sharing: Sharing::default(),
        },
    )
    .await
    .unwrap();
    let board_id: BoardId = board.full.board.id;
    let sprint = sprints_service::create(
        pool,
        &admin,
        WEB,
        board_id,
        NewSprint {
            name: "Sprint One".to_owned(),
            ..NewSprint::default()
        },
        T0,
    )
    .await
    .unwrap();

    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &input("board \"Board One\" \"Sprint One\"", &[target]),
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert_eq!(
        taskmaster_store::boards::cards::places(pool, board_id, target)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        taskmaster_store::boards::cards::places(pool, board_id, target)
            .await
            .unwrap()[0]
            .sprint,
        sprint.sprint.id
    );

    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &input("remove board \"Board One\"", &[target]),
    )
    .await
    .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert!(
        taskmaster_store::boards::cards::places(pool, board_id, target)
            .await
            .unwrap()
            .is_empty()
    );
}

/// cmd_020: `delete` marks the issue deleted (CMD-020); the whole line is one transaction.
#[tokio::test]
async fn cmd_020_delete_command() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(pool, &admin, WEB, &input("state Fixed delete", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let gone = sqlx::query_scalar!(
        r#"select deleted_at is not null as "gone!" from issues where id = $1"#,
        target
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(gone);
    // The state write went with it, in the same transaction: its record joined the history.
    let recorded: i64 = sqlx::query_scalar!(
        r#"select count(*) as "n!" from issue_history where issue_id = $1"#,
        target
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(recorded >= 2, "{recorded}");
}

/// cmd_021: `clone` makes a published copy of the same project with a «(copy)» summary
/// (CMD-021, AUT-066); the message names the copy.
#[tokio::test]
async fn cmd_021_clone_command() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(pool, &admin, WEB, &input("clone", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert!(
        answer.issues[0]
            .messages
            .iter()
            .any(|message| message.code == "clone_created")
    );
    let copies: i64 = sqlx::query_scalar!(
        r#"select count(*) as "n!" from issues where project_id = $1 and summary = 'Commanded (copy)'"#,
        web
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(copies, 1);
}

/// cmd_025: silence needs `issue.silent` in every affected project (CMD-025, plan §4.4); with
/// the right, the history records the change as silent.
#[tokio::test]
async fn cmd_025_silent_needs_the_right() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "bob", web).await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(
        pool,
        &bob,
        WEB,
        &ApplyInput {
            silent: true,
            ..input("state Fixed", &[target])
        },
    )
    .await
    .unwrap();
    assert_eq!(codes(&answer), [CommandCode::CmdNoPermission]);
    assert_eq!(
        answer.plan.problems[0]
            .params
            .get("permission")
            .map(String::as_str),
        Some("issue.silent")
    );

    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &ApplyInput {
            silent: true,
            ..input("state Fixed", &[target])
        },
    )
    .await
    .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let silent: bool = sqlx::query_scalar!(
        r#"select exists(select 1 from issue_history where issue_id = $1 and silent) as "s!""#,
        target
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(silent);
}

/// cmd_026: the bulk job walks the query's issues, applies per issue and reports progress and
/// failures (CMD-026, plan §4.7).
#[tokio::test]
async fn cmd_026_bulk_job_applies_and_reports() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let mut issues: Vec<IssueId> = Vec::new();
    for _ in 0..3 {
        issues.push(issue(pool, &admin, web).await);
    }

    let mut handlers = Handlers::new();
    handlers.add::<bulk::BulkWork, _, _>({
        let pool = taskmaster_store::PgPool::clone(pool);
        move |context, job| {
            let pool = pool.clone();
            async move { bulk::run(&pool, &context, job).await }
        }
    });
    let runner = Runner::start(
        pool.clone(),
        handlers,
        RunnerConfig {
            workers: 1,
            poll_interval: Duration::from_millis(20),
            ..RunnerConfig::default()
        },
        None,
    );
    let queued = bulk::enqueue(
        pool,
        &admin,
        bulk::BulkWork {
            actor: admin.user(),
            text: "state Fixed".to_owned(),
            query: String::new(),
            project: Some(web),
            hide_resolved: false,
            time_zone: None,
            comment: None,
            comment_visibility: None,
            silent: false,
            create_missing: false,
            run_as: None,
        },
    )
    .await
    .unwrap();
    let mut done = None;
    for _ in 0..200 {
        if let Some(job) = jobs_queue::get(pool, queued.id).await.unwrap()
            && job.state.is_final()
        {
            done = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    runner.shutdown().await;
    let job = done.expect("the bulk job never finished");
    assert_eq!(job.state, JobState::Finished, "{:?}", job.last_error);

    let progress = bulk::progress(pool, &admin, queued.id).await.unwrap();
    assert_eq!(progress.state, "finished");
    assert_eq!(progress.done, 3);
    assert_eq!(progress.total, Some(3));
    assert_eq!(progress.failed, 0);
    for issue in &issues {
        assert_eq!(
            names_of(pool, &admin, *issue, "State").await,
            ["Fixed".to_owned()]
        );
    }

    // Not the owner's: the progress is the owner's alone.
    let outsider = member(pool, &admin, "eve", web).await;
    assert!(bulk::progress(pool, &outsider, queued.id).await.is_err());
}

/// cmd_031: `run_as` needs `system.write`, audits both users, history keeps the acting user and
/// only the permission checks answer as the named account (CMD-031, plan §4.6).
#[tokio::test]
async fn cmd_031_run_as_another_user() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "bob", web).await;
    let target = issue(pool, &admin, web).await;

    // A member cannot run as anyone.
    let error = commands::apply(
        pool,
        &bob,
        WEB,
        &ApplyInput {
            run_as: Some(admin.user()),
            ..input("state Fixed", &[target])
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Forbidden(_)));

    // The administrator can, in the named account's rights; history keeps the acting user.
    let answer = commands::apply(
        pool,
        &admin,
        WEB,
        &ApplyInput {
            run_as: Some(bob.user()),
            ..input("state Fixed", &[target])
        },
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    let author: UserId = sqlx::query_scalar!(
        "select author_id from issue_history where issue_id = $1 limit 1",
        target
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(author, admin.user());
    let audited: i64 = sqlx::query_scalar!(
        r#"select count(*) as "n!" from audit_events where action = 'commands.run_as'"#
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(audited, 1);
}

/// cmd_032: the Russian command words are always active (CMD-032, D-013) — the names come from
/// the instance's own language, the words from the vocabulary.
#[tokio::test]
async fn cmd_032_russian_command_words() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "bob", web).await;
    let target = issue(pool, &admin, web).await;

    let answer = commands::apply(pool, &bob, WEB, &input("следить", &[target]))
        .await
        .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert_eq!(
        taskmaster_store::issues::watched(pool, &[target], bob.user())
            .await
            .unwrap(),
        [target]
    );

    let answer = commands::apply(
        pool,
        &bob,
        WEB,
        &ApplyInput {
            comment: Some("Готово.".to_owned()),
            ..input("комментарий", &[target])
        },
    )
    .await
    .unwrap();
    assert!(answer.issues[0].ok, "{:?}", answer.issues[0].problem);
    assert_eq!(comment_count(pool, target).await, 1);
}

// ---- 2.1b: the assist (plan §5) ----

use taskmaster_query::completion::Group;
use taskmaster_service::assist;

fn assist_input(text: &str, issues: &[IssueId], caret: usize) -> assist::CommandAssist {
    assist::CommandAssist {
        text: text.to_owned(),
        caret,
        issues: issues.to_vec(),
        ..assist::CommandAssist::default()
    }
}

/// The preview (CMD-028): the plan's operations carry the names they resolved to, the problems
/// sit over their ranges, and a value the field lacks brings its create fix along.
#[tokio::test]
async fn cmd_028_preview_problems_and_quick_fixes() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let target = issue(pool, &admin, web).await;

    // A clean line: one operation, with what it names as the preview's row shows it.
    let answer = assist::commands(
        pool,
        &admin,
        &assist_input("state In progress", &[target], 17),
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(answer.plan.operations.len(), 1);
    assert_eq!(answer.plan.operations[0].names, ["State", "In progress"]);

    // A value the field lacks: the problem over the value's range, the fix riding it.
    let answer = assist::commands(pool, &admin, &assist_input("state Broke", &[target], 11))
        .await
        .unwrap();
    assert_eq!(
        answer
            .plan
            .problems
            .iter()
            .map(|problem| problem.code)
            .collect::<Vec<_>>(),
        [CommandCode::CmdUnknownValue]
    );
    let problem = &answer.plan.problems[0];
    assert_eq!((problem.span.start, problem.span.end), (6, 11));
    let fix = problem.quick_fix.as_ref().unwrap();
    assert_eq!(fix.name, "Broke");
    assert_eq!(fix.projects, [web]);

    // The comment command without its text: the problem says so over the word.
    let answer = assist::commands(pool, &admin, &assist_input("comment", &[target], 7))
        .await
        .unwrap();
    assert_eq!(
        answer
            .plan
            .problems
            .iter()
            .map(|problem| problem.code)
            .collect::<Vec<_>>(),
        [CommandCode::CmdValueExpected]
    );
}

/// The completion (CMD-029): each context offers its own groups, in both languages, values are
/// labelled with their field and marked when the targets already carry them (§1), relative
/// dates answer their concrete value, issues follow a link word, and users follow the
/// directory right (QRY-057's rule, one dictionary).
#[tokio::test]
async fn cmd_029_completion_by_context() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    value_of(pool, &admin, web, "Fix versions", "2025.3").await;
    let target = issue(pool, &admin, web).await;
    issue(pool, &admin, web).await;
    teams::add(pool, &admin, web, &[bob.user()], &[])
        .await
        .unwrap();

    // An empty line: the command words, both languages.
    let answer = assist::commands(pool, &bob, &assist_input("", &[target], 0))
        .await
        .unwrap();
    let words: Vec<&str> = answer
        .suggestions
        .iter()
        .filter(|s| s.group == Group::Command)
        .map(|s| s.text.as_str())
        .collect();
    assert!(words.contains(&"add"), "{words:?}");
    assert!(words.contains(&"добавить"), "{words:?}");

    // After a field: its values, labelled with it; the one the target already carries is
    // marked present.
    commands::apply(pool, &bob, WEB, &input("state In progress", &[target]))
        .await
        .unwrap();
    let answer = assist::commands(pool, &bob, &assist_input("state ", &[target], 6))
        .await
        .unwrap();
    // Multi-word names insert quoted (QRY-056's auto-quoting).
    let found = answer
        .suggestions
        .iter()
        .find(|s| s.text == "\"In progress\"")
        .unwrap();
    assert_eq!(found.description, "State");
    assert!(found.present);
    assert!(
        answer
            .suggestions
            .iter()
            .all(|s| matches!(s.group, Group::Value | Group::Empty))
    );
    // A partial multi-word value replaces the run and arrives quoted.
    let answer = assist::commands(pool, &bob, &assist_input("state in pr", &[target], 11))
        .await
        .unwrap();
    assert_eq!(answer.suggestions[0].text, "\"In progress\"");
    assert_eq!(
        (
            answer.suggestions[0].replace.start,
            answer.suggestions[0].replace.end
        ),
        (6, 11)
    );

    // After `add`: the multi-value fields and their values, never the single-value fields'.
    let answer = assist::commands(pool, &bob, &assist_input("add ", &[target], 4))
        .await
        .unwrap();
    let fields: Vec<&str> = answer
        .suggestions
        .iter()
        .filter(|s| s.group == Group::Attribute)
        .map(|s| s.text.as_str())
        .collect();
    assert!(fields.contains(&"Fix versions"), "{fields:?}");
    assert!(!fields.contains(&"State"), "{fields:?}");
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.text == "2025.3" && s.description == "Fix versions")
    );

    // Tags, in both languages: the one the target carries is marked.
    taskmaster_service::tags::create(pool, &bob, "web", None)
        .await
        .unwrap();
    commands::apply(pool, &bob, WEB, &input("тег web", &[target]))
        .await
        .unwrap();
    let answer = assist::commands(pool, &bob, &assist_input("untag ", &[target], 6))
        .await
        .unwrap();
    let found = answer.suggestions.iter().find(|s| s.text == "web").unwrap();
    assert_eq!(found.group, Group::Tag);
    assert!(found.present);

    // Users follow the directory right (QRY-057): everyone holds it through All members
    // (ACC-021), and without the role an account names only itself.
    let answer = assist::commands(pool, &bob, &assist_input("assignee ", &[target], 9))
        .await
        .unwrap();
    let users: Vec<&str> = answer
        .suggestions
        .iter()
        .filter(|s| s.group == Group::User)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(users, ["admin", "Bob"]);
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.group == Group::User && s.own)
    );
    let all_members = taskmaster_store::groups::builtin_ids(pool).await.unwrap().1;
    let assignment = roles::assignments_of_holder(pool, &admin, Holder::Group(all_members))
        .await
        .unwrap()[0]
        .id;
    roles::revoke(pool, &admin, assignment).await.unwrap();
    let bob = access::load(pool, bob.user()).await.unwrap();
    let answer = assist::commands(pool, &bob, &assist_input("assignee ", &[target], 9))
        .await
        .unwrap();
    let users: Vec<&str> = answer
        .suggestions
        .iter()
        .filter(|s| s.group == Group::User)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(users, ["Bob"]);
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.group == Group::User && s.own)
    );

    // After a link word: issues, by their readable ID with the summary.
    let answer = assist::commands(pool, &bob, &assist_input("related to ", &[target], 12))
        .await
        .unwrap();
    let issues: Vec<(&str, &str)> = answer
        .suggestions
        .iter()
        .filter(|s| s.group == Group::Issue)
        .map(|s| (s.text.as_str(), s.description.as_str()))
        .collect();
    assert!(issues.contains(&("WEB-2", "Commanded")), "{issues:?}");

    // A date field: the words answer their concrete value in the caller's zone.
    taskmaster_service::project_fields::attach(
        pool,
        &admin,
        web,
        taskmaster_service::project_fields::AttachInput::New {
            name: "Deadline".to_owned(),
            kind: taskmaster_domain::fields::FieldKind::Date,
            multi: false,
            aliases: Vec::new(),
        },
    )
    .await
    .unwrap();
    let answer = assist::commands(pool, &bob, &assist_input("deadline ", &[target], 9))
        .await
        .unwrap();
    let today = answer
        .suggestions
        .iter()
        .find(|s| s.text == "today")
        .unwrap();
    assert_eq!(today.group, Group::Date);
    let resolved = today.resolved.as_deref().unwrap();
    assert_eq!(
        resolved,
        time::OffsetDateTime::now_utc().date().to_string().as_str()
    );
}

/// cmd_030: the lines a person applied land in the history (plan §6) — a repeat bumps its time,
/// the trim keeps 20, a line whose plan had problems never lands — and the history leads the
/// completion's `recent` group on a fresh command.
#[tokio::test]
async fn cmd_030_history_feeds_the_recent_group() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    let target = issue(pool, &admin, web).await;

    // An applied line is recorded; a line with problems is not.
    commands::apply(pool, &bob, WEB, &input("state In progress", &[target]))
        .await
        .unwrap();
    commands::apply(pool, &bob, WEB, &input("state Nope", &[target]))
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let mut lines = taskmaster_store::command_history::recent(&mut *conn, bob.user(), 20)
        .await
        .unwrap();
    assert_eq!(lines, ["state In progress"]);

    // The trim keeps the newest 20; a repeat bumps its line to the front.
    for n in 0..20 {
        taskmaster_store::command_history::record(&mut conn, bob.user(), &format!("tag t{n}"))
            .await
            .unwrap();
    }
    taskmaster_store::command_history::record(&mut conn, bob.user(), "state In progress")
        .await
        .unwrap();
    lines = taskmaster_store::command_history::recent(&mut *conn, bob.user(), 20)
        .await
        .unwrap();
    assert_eq!(lines.len(), taskmaster_store::command_history::KEEP);
    assert_eq!(lines[0], "state In progress");
    assert!(!lines.contains(&"state Nope".to_owned()));
    // The oldest of the overflow fell out; the rest stayed.
    assert!(!lines.contains(&"tag t0".to_owned()));
    assert!(lines.contains(&"tag t1".to_owned()));

    // The completion offers the person's lines as the `recent` group, first at a fresh command.
    let answer = assist::commands(pool, &bob, &assist_input("", &[target], 0))
        .await
        .unwrap();
    let recent = answer
        .suggestions
        .iter()
        .find(|s| s.group == Group::Recent)
        .unwrap();
    assert_eq!(recent.text, "state In progress");
    assert_eq!(answer.suggestions[0].group, Group::Recent);
}

/// cmd_028: the preview's marks (plan §6) — the direction of every operation, and the values a
/// plain set on a multi-value field takes off the target (§1: the preview shows the removals).
#[tokio::test]
async fn cmd_028_preview_marks_the_removals() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let bob = member(pool, &admin, "Bob", web).await;
    for name in ["2025.1", "2025.2", "2025.3"] {
        value_of(pool, &admin, web, "Fix versions", name).await;
    }
    let target = issue(pool, &admin, web).await;
    commands::apply(
        pool,
        &bob,
        WEB,
        &input("add fix versions 2025.1", &[target]),
    )
    .await
    .unwrap();
    commands::apply(
        pool,
        &bob,
        WEB,
        &input("add fix versions 2025.2", &[target]),
    )
    .await
    .unwrap();

    // A plain set replaces the list: the marks name what stays off.
    let answer = assist::commands(
        pool,
        &bob,
        &assist_input("fix versions 2025.3", &[target], 18),
    )
    .await
    .unwrap();
    assert!(
        answer.plan.problems.is_empty(),
        "{:?}",
        answer.plan.problems
    );
    assert_eq!(answer.marks.len(), 1);
    assert_eq!(answer.marks[0].mode, Some("set"));
    let mut removals = answer.marks[0].removals.clone();
    removals.sort();
    assert_eq!(removals, ["2025.1", "2025.2"]);

    // `add` and `remove` carry their direction and no removals of their own.
    let answer = assist::commands(
        pool,
        &bob,
        &assist_input("add fix versions 2025.3", &[target], 22),
    )
    .await
    .unwrap();
    assert_eq!(answer.marks[0].mode, Some("add"));
    assert!(answer.marks[0].removals.is_empty());
}
