//! Sprints and membership modes (BRD-024, BRD-026…BRD-030, BRD-034, BRD-097…BRD-104; plan step
//! 1.8 B2, B3), the header filter and its sort (BRD-070, BRD-081) on a real database.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::access::Access;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::boards::query_mode::Step;
use taskmaster_domain::boards::{Mode, Preset};
use taskmaster_domain::issues::history::Origin;
use taskmaster_domain::issues::values::Input;
use taskmaster_domain::projects::Template;
use taskmaster_domain::settings::Language;
use taskmaster_domain::sharing::Audience;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{BoardId, IssueId, ProjectId, SprintId};
use taskmaster_domain::{ColumnId, FieldId};
use taskmaster_service::boards::backlog::{self, BacklogInput};
use taskmaster_service::boards::cards::{self, Move, NewCard};
use taskmaster_service::boards::membership::{self, Target};
use taskmaster_service::boards::sprints::{self, NewSprint, SprintChange};
use taskmaster_service::boards::{self as boards, BoardChange, BoardRow, NewBoard, Sharing, view};
use taskmaster_service::issues::{self, FieldChange, NewIssue};
use taskmaster_service::projects::{self, NewProjectInput};
use taskmaster_service::search::{self, SearchInput};
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::users::{self, NewAccount};
use taskmaster_service::{Error, Passwords, access, roles};
use taskmaster_store::PgPool;
use taskmaster_store::access::Holder;
use taskmaster_store::sessions::Origin as SessionOrigin;
use taskmaster_testkit::TestDb;
use time::OffsetDateTime;
use time::macros::{date, datetime};

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const NOW: OffsetDateTime = datetime!(2026-10-24 12:00 UTC);

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

async fn project(pool: &PgPool, actor: &Access, key: &str) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Scrum,
            name: format!("Project {key}"),
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

async fn member(pool: &PgPool, admin: &Access, username: &str, of: &[ProjectId]) -> Access {
    let user = users::create(
        pool,
        &passwords(),
        admin,
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
    let role = taskmaster_store::roles::builtin_id(pool, PredefinedRole::ProjectMember.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().map(|p| Some(*p)).collect();
    roles::assign(pool, admin, role, Holder::User(user), &scopes)
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

async fn new_issue(pool: &PgPool, actor: &Access, project: ProjectId, publish: bool) -> IssueId {
    issues::create(
        pool,
        actor,
        Origin::Web,
        NewIssue {
            project,
            summary: "Something".to_owned(),
            publish,
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

async fn issue(pool: &PgPool, actor: &Access, project: ProjectId) -> IssueId {
    new_issue(pool, actor, project, true).await
}

async fn board(pool: &PgPool, who: &Access, preset: Preset, projects: &[ProjectId]) -> BoardRow {
    boards::create(
        pool,
        who,
        Language::En,
        NewBoard {
            preset,
            name: "Team".to_owned(),
            projects: projects.to_vec(),
            sharing: Sharing::default(),
        },
    )
    .await
    .unwrap()
}

async fn reload(pool: &PgPool, who: &Access, id: BoardId) -> BoardRow {
    boards::get(pool, who, id).await.unwrap()
}

async fn change(
    pool: &PgPool,
    who: &Access,
    id: BoardId,
    c: BoardChange,
) -> Result<BoardRow, Error> {
    let now = reload(pool, who, id).await;
    boards::update(
        pool,
        who,
        Language::En,
        id,
        BoardChange {
            version: now.full.board.version,
            ..c
        },
    )
    .await
}

fn named(name: &str) -> NewSprint {
    NewSprint {
        name: name.to_owned(),
        ..NewSprint::default()
    }
}

async fn sprint(pool: &PgPool, who: &Access, board: BoardId, input: NewSprint) -> SprintId {
    sprints::create(pool, who, Origin::Web, board, input, NOW)
        .await
        .unwrap()
        .sprint
        .id
}

fn first_sprint(b: &BoardRow) -> SprintId {
    b.full.board.sprints_on.then_some(()).unwrap();
    b.full.sprints[0].id
}

async fn add(pool: &PgPool, who: &Access, board: BoardId, sprint: SprintId, issue: IssueId) {
    taskmaster_service::boards::cards::add(
        pool,
        who,
        Origin::Web,
        board,
        sprint,
        NewCard {
            issue,
            ..NewCard::default()
        },
    )
    .await
    .unwrap();
}

/// The sprints that hold the issue.
async fn sprints_of(pool: &PgPool, issue: IssueId) -> Vec<SprintId> {
    sqlx::query_scalar("select sprint_id from sprint_issues where issue_id = $1 order by sprint_id")
        .bind(issue)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn board_records(
    pool: &PgPool,
    issue: IssueId,
) -> Vec<(Option<serde_json::Value>, Option<serde_json::Value>)> {
    sqlx::query_as(
        "select removed, added from issue_history where issue_id = $1 and kind = 'boards'
         order by id",
    )
    .bind(issue)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn resolve(pool: &PgPool, issue: IssueId) {
    sqlx::query("update issues set resolved_at = now() where id = $1")
        .bind(issue)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn brd_097_schedule_rules() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let bad = sprints::create(
        pool,
        &admin,
        Origin::Web,
        id,
        NewSprint {
            starts_on: Some(date!(2026 - 10 - 20)),
            ends_on: Some(date!(2026 - 10 - 10)),
            ..named("Late")
        },
        NOW,
    )
    .await;
    assert!(matches!(bad, Err(Error::Invalid(ref p)) if p.iter().any(|f| f.path == "ends_on")));
    let open = sprints::create(
        pool,
        &admin,
        Origin::Web,
        id,
        NewSprint {
            starts_on: Some(date!(2026 - 10 - 20)),
            ..named("Open end")
        },
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(open.sprint.ends_on, None);
    assert_eq!(
        open.current,
        Some(open.sprint.id),
        "a started sprint beats unscheduled ones"
    );
}

#[tokio::test]
async fn brd_098_create() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let leo = member(pool, &admin, "leo", &[p]).await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    assert_eq!(b.full.sprints[0].name, "Sprint 1");
    let created = sprints::create(
        pool,
        &admin,
        Origin::Web,
        id,
        NewSprint {
            goal: " Ship it ".to_owned(),
            ..named("Sprint 2")
        },
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(created.sprint.goal, "Ship it");
    assert_eq!(created.sprint.starts_on, None, "unscheduled by default");
    let taken = sprints::create(pool, &admin, Origin::Web, id, named("sprint 2"), NOW).await;
    assert!(
        matches!(taken, Err(Error::Invalid(ref p)) if p.iter().any(|f| f.problem == Problem::Taken))
    );
    // Seeing the board is not enough: sprints are settings (BRD-098).
    change(
        pool,
        &admin,
        id,
        BoardChange {
            sharing: Some(Sharing {
                view: Audience {
                    users: vec![leo.user()],
                    groups: Vec::new(),
                },
                ..Sharing::default()
            }),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        sprints::create(pool, &leo, Origin::Web, id, named("Mine"), NOW).await,
        Err(Error::Refused(_))
    ));
    // A sprintless board has none to add.
    let k = board(pool, &admin, Preset::Kanban, &[p]).await;
    assert!(matches!(
        sprints::create(pool, &admin, Origin::Web, k.full.board.id, named("X"), NOW).await,
        Err(Error::Refused(_))
    ));
    let list = sprints::list(pool, &admin, k.full.board.id, NOW)
        .await
        .unwrap();
    assert!(list.sprints.is_empty(), "the hidden sprint is not listed");
}

#[tokio::test]
async fn brd_098_auto_target_and_carry_on_create() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    let open = issue(pool, &admin, p).await;
    let done = issue(pool, &admin, p).await;
    add(pool, &admin, id, s1, open).await;
    add(pool, &admin, id, s1, done).await;
    resolve(pool, done).await;
    let s2 = sprint(
        pool,
        &admin,
        id,
        NewSprint {
            carry_from: Some(s1),
            auto_target: true,
            ..named("Sprint 2")
        },
    )
    .await;
    assert_eq!(sprints_of(pool, open).await, vec![s2]);
    assert_eq!(sprints_of(pool, done).await, vec![s1], "resolved ones stay");
    let now = reload(pool, &admin, id).await;
    assert_eq!(now.full.board.auto_sprint_id, Some(s2));
    assert_eq!(Mode::parse(&now.full.board.mode), Some(Mode::Auto));
}

#[tokio::test]
async fn brd_099_start_makes_current() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    let s2 = sprint(pool, &admin, id, named("Sprint 2")).await;
    let v = view::view(pool, &admin, id, None, None, None, Some(NOW))
        .await
        .unwrap();
    assert_eq!(v.current, Some(s1), "all unscheduled: the earliest created");
    let started = sprints::start(pool, &admin, id, s2, NOW).await.unwrap();
    assert_eq!(started.sprint.starts_on, Some(date!(2026 - 10 - 24)));
    assert_eq!(started.current, Some(s2));
}

#[tokio::test]
async fn brd_100_finish_by_hand_and_by_date() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    let s2 = sprint(
        pool,
        &admin,
        id,
        NewSprint {
            starts_on: Some(date!(2026 - 10 - 01)),
            ends_on: Some(date!(2026 - 10 - 24)),
            ..named("Sprint 2")
        },
    )
    .await;
    let row = sprints::finish(pool, &admin, id, s1, NOW).await.unwrap();
    assert!(row.sprint.finished_at.is_some());
    assert_eq!(row.current, Some(s2), "the last day still runs");
    let next_day = NOW + time::Duration::days(1);
    let list = sprints::list(pool, &admin, id, next_day).await.unwrap();
    assert_eq!(list.current, None, "its end passed");
    // Starting again undoes the finish (D-013).
    let again = sprints::start(pool, &admin, id, s1, NOW).await.unwrap();
    assert!(again.sprint.finished_at.is_none());
    assert_eq!(again.current, Some(s2), "s2 started earlier");
}

#[tokio::test]
async fn brd_101_carry_over() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    let s2 = sprint(pool, &admin, id, named("Sprint 2")).await;
    let a = issue(pool, &admin, p).await;
    let c = issue(pool, &admin, p).await;
    let done = issue(pool, &admin, p).await;
    let there = issue(pool, &admin, p).await;
    for i in [a, c, done] {
        add(pool, &admin, id, s1, i).await;
    }
    add(pool, &admin, id, s2, there).await;
    resolve(pool, done).await;
    sprints::finish(pool, &admin, id, s1, NOW).await.unwrap();
    let row = sprints::carry(pool, &admin, Origin::Web, id, s1, s2, NOW)
        .await
        .unwrap();
    assert_eq!(row.sprint.id, s2);
    assert_eq!(sprints_of(pool, a).await, vec![s2]);
    assert_eq!(sprints_of(pool, c).await, vec![s2]);
    assert_eq!(sprints_of(pool, done).await, vec![s1]);
    // They follow the cards already there, in their old order.
    let order: Vec<IssueId> = sqlx::query_scalar(
        "select issue_id from sprint_issues where sprint_id = $1 order by rank, issue_id",
    )
    .bind(s2)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(order, vec![there, a, c]);
    let records = board_records(pool, a).await;
    let last = records.last().unwrap();
    assert_eq!(last.0.as_ref().unwrap()["sprint"]["id"], s1);
    assert_eq!(last.1.as_ref().unwrap()["sprint"]["id"], s2);
    assert!(matches!(
        sprints::carry(pool, &admin, Origin::Web, id, s2, s2, NOW).await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn brd_102_edit() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = &b.full.sprints[0];
    let edited = sprints::update(
        pool,
        &admin,
        id,
        s1.id,
        SprintChange {
            name: Some("Kickoff".to_owned()),
            goal: Some("Learn".to_owned()),
            starts_on: Some(Some(date!(2026 - 10 - 20))),
            ends_on: Some(Some(date!(2026 - 11 - 03))),
            version: s1.version,
        },
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(edited.sprint.name, "Kickoff");
    assert_eq!(edited.sprint.goal, "Learn");
    assert_eq!(edited.sprint.ends_on, Some(date!(2026 - 11 - 03)));
    let stale = sprints::update(
        pool,
        &admin,
        id,
        s1.id,
        SprintChange {
            ends_on: Some(None),
            version: s1.version,
            ..SprintChange::default()
        },
        NOW,
    )
    .await;
    assert!(matches!(stale, Err(Error::Conflict(_))));
    let cleared = sprints::update(
        pool,
        &admin,
        id,
        s1.id,
        SprintChange {
            ends_on: Some(None),
            version: edited.sprint.version,
            ..SprintChange::default()
        },
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(cleared.sprint.ends_on, None);
    assert_eq!(cleared.sprint.name, "Kickoff", "absent keeps");
}

#[tokio::test]
async fn brd_103_archive_and_restore() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    let s2 = sprint(
        pool,
        &admin,
        id,
        NewSprint {
            auto_target: true,
            ..named("Sprint 2")
        },
    )
    .await;
    let archived = sprints::archive(pool, &admin, id, s1, true, NOW)
        .await
        .unwrap();
    assert!(archived.sprint.archived_at.is_some());
    assert_eq!(
        archived.current,
        Some(s2),
        "archived sprints leave detection"
    );
    sprints::archive(pool, &admin, id, s2, true, NOW)
        .await
        .unwrap();
    assert_eq!(
        reload(pool, &admin, id).await.full.board.auto_sprint_id,
        None,
        "an archived sprint is no target"
    );
    let list = sprints::list(pool, &admin, id, NOW).await.unwrap();
    assert_eq!(list.sprints.len(), 2, "archived ones are listed");
    assert_eq!(list.current, None);
    let restored = sprints::archive(pool, &admin, id, s1, false, NOW)
        .await
        .unwrap();
    assert!(restored.sprint.archived_at.is_none());
    assert_eq!(restored.current, Some(s1));
}

#[tokio::test]
async fn brd_104_delete() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    assert!(matches!(
        sprints::delete(pool, &admin, id, s1).await,
        Err(Error::Refused(_))
    ));
    let s2 = sprint(pool, &admin, id, named("Sprint 2")).await;
    let i = issue(pool, &admin, p).await;
    add(pool, &admin, id, s2, i).await;
    sprints::delete(pool, &admin, id, s2).await.unwrap();
    assert!(sprints_of(pool, i).await.is_empty());
    assert_eq!(board_records(pool, i).await.len(), 1, "history stays");
    assert!(matches!(
        sprints::delete(pool, &admin, id, s2).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn brd_024_switch_off_and_on() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    let s2 = sprint(pool, &admin, id, named("Sprint 2")).await;
    sprints::start(pool, &admin, id, s2, NOW).await.unwrap();
    let i = issue(pool, &admin, p).await;
    add(pool, &admin, id, s2, i).await;
    let off = change(
        pool,
        &admin,
        id,
        BoardChange {
            sprints_on: Some(false),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert!(!off.full.board.sprints_on);
    let hidden = off.full.sprints.iter().find(|s| s.internal).unwrap();
    assert_eq!(hidden.id, s2, "the current sprint holds the cards");
    let other = off.full.sprints.iter().find(|s| s.id == s1).unwrap();
    assert!(other.archived_at.is_some(), "the others are archived");
    let v = view::view(pool, &admin, id, None, None, None, Some(NOW))
        .await
        .unwrap();
    assert_eq!(v.cards.iter().map(|c| c.issue).collect::<Vec<_>>(), vec![i]);
    let on = change(
        pool,
        &admin,
        id,
        BoardChange {
            sprints_on: Some(true),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let back = on.full.sprints.iter().find(|s| s.id == s2).unwrap();
    assert!(!back.internal);
    assert_eq!(
        back.name, "Sprint 2",
        "the first free number after the others"
    );
    assert!(on.full.sprints.iter().all(|s| !s.internal));
}

#[tokio::test]
async fn brd_024_switch_on_names_after_others() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let k = board(pool, &admin, Preset::Kanban, &[p]).await;
    let on = change(
        pool,
        &admin,
        k.full.board.id,
        BoardChange {
            sprints_on: Some(true),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(on.full.sprints.len(), 1);
    assert_eq!(on.full.sprints[0].name, "Sprint 1");
    assert!(!on.full.sprints[0].internal);
}

#[tokio::test]
async fn brd_026_auto_target() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    change(
        pool,
        &admin,
        id,
        BoardChange {
            mode: Some(Mode::Auto),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let before = issue(pool, &admin, p).await;
    assert!(
        sprints_of(pool, before).await.is_empty(),
        "no target: nothing joins"
    );
    change(
        pool,
        &admin,
        id,
        BoardChange {
            auto_sprint: Some(Some(s1)),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let after = issue(pool, &admin, p).await;
    assert_eq!(sprints_of(pool, after).await, vec![s1]);
    let records = board_records(pool, after).await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].1.as_ref().unwrap()["sprint"]["id"], s1);
    let draft = new_issue(pool, &admin, p, false).await;
    assert!(sprints_of(pool, draft).await.is_empty(), "drafts wait");
    issues::publish(pool, &admin, Origin::Web, draft)
        .await
        .unwrap();
    assert_eq!(sprints_of(pool, draft).await, vec![s1]);
    // The target must be a visible sprint of the board.
    assert!(matches!(
        change(
            pool,
            &admin,
            id,
            BoardChange {
                auto_sprint: Some(Some(s1 + 1000)),
                ..BoardChange::default()
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        change(
            pool,
            &admin,
            id,
            BoardChange {
                mode: Some(Mode::Query),
                ..BoardChange::default()
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn brd_027_auto_sprintless() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let other = project(pool, &admin, "OT").await;
    let k = board(pool, &admin, Preset::Kanban, &[p]).await;
    let id = k.full.board.id;
    let hidden = k.full.sprints[0].id;
    change(
        pool,
        &admin,
        id,
        BoardChange {
            mode: Some(Mode::Auto),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let i = issue(pool, &admin, p).await;
    assert_eq!(sprints_of(pool, i).await, vec![hidden]);
    let record = &board_records(pool, i).await[0];
    assert!(record.1.as_ref().unwrap().get("sprint").is_none());
    let outside = issue(pool, &admin, other).await;
    assert!(sprints_of(pool, outside).await.is_empty());
    // A draft kept off the board (ISS-007) does not join, and forgets it.
    let draft = new_issue(pool, &admin, p, false).await;
    sqlx::query("update issues set board_skips = $2 where id = $1")
        .bind(draft)
        .bind(vec![id])
        .execute(pool)
        .await
        .unwrap();
    issues::publish(pool, &admin, Origin::Web, draft)
        .await
        .unwrap();
    assert!(sprints_of(pool, draft).await.is_empty());
    let skips: Vec<i64> = sqlx::query_scalar("select board_skips from issues where id = $1")
        .bind(draft)
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(skips.is_empty());
}

#[tokio::test]
async fn brd_027_skips_unknown_column() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let k = board(pool, &admin, Preset::Kanban, &[p]).await;
    let id = k.full.board.id;
    change(
        pool,
        &admin,
        id,
        BoardChange {
            mode: Some(Mode::Auto),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    sqlx::query("delete from board_columns where board_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    let i = issue(pool, &admin, p).await;
    assert!(
        sprints_of(pool, i).await.is_empty(),
        "no column for its value"
    );
}

#[tokio::test]
async fn brd_034_targets() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = board(pool, &admin, Preset::Scrum, &[p]).await;
    let id = b.full.board.id;
    let s1 = first_sprint(&b);
    let s2 = sprint(pool, &admin, id, named("Sprint 2")).await;
    let i = issue(pool, &admin, p).await;
    let card = NewCard {
        issue: i,
        ..NewCard::default()
    };
    assert!(matches!(
        membership::add(
            pool,
            &admin,
            Origin::Web,
            id,
            Target::Auto,
            card.clone(),
            NOW
        )
        .await,
        Err(Error::Refused(_))
    ));
    membership::add(
        pool,
        &admin,
        Origin::Web,
        id,
        Target::Current,
        card.clone(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(sprints_of(pool, i).await, vec![s1]);
    change(
        pool,
        &admin,
        id,
        BoardChange {
            auto_sprint: Some(Some(s2)),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    membership::add(
        pool,
        &admin,
        Origin::Web,
        id,
        Target::Auto,
        card.clone(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(sprints_of(pool, i).await, vec![s2], "one sprint per board");
    membership::remove_from_board(pool, &admin, Origin::Web, id, i)
        .await
        .unwrap();
    assert!(sprints_of(pool, i).await.is_empty());
    assert!(matches!(
        membership::remove_from_board(pool, &admin, Origin::Web, id, i).await,
        Err(Error::NotFound(_))
    ));
    // Every sprint is finished: no current one.
    sprints::finish(pool, &admin, id, s1, NOW).await.unwrap();
    sprints::finish(pool, &admin, id, s2, NOW).await.unwrap();
    assert!(matches!(
        membership::add(
            pool,
            &admin,
            Origin::Web,
            id,
            Target::Current,
            card.clone(),
            NOW
        )
        .await,
        Err(Error::Refused(_))
    ));
    // A sprintless board has one place: any target means it.
    let k = board(pool, &admin, Preset::Kanban, &[p]).await;
    membership::add(
        pool,
        &admin,
        Origin::Web,
        k.full.board.id,
        Target::Current,
        card,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(sprints_of(pool, i).await, vec![k.full.sprints[0].id]);
}

// --- Query mode (BRD-028…BRD-030), the header filter and its sort (BRD-070, BRD-081) ---

async fn field(pool: &PgPool, name: &str) -> FieldId {
    sqlx::query_scalar("select id from fields where name = $1 order by id limit 1")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn value(pool: &PgPool, project: ProjectId, field: FieldId, name: &str) -> i64 {
    sqlx::query_scalar(
        "select v.id from field_values v join project_fields pf on pf.set_id = v.set_id
         where pf.project_id = $1 and pf.field_id = $2 and v.name = $3",
    )
    .bind(project)
    .bind(field)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn set(pool: &PgPool, who: &Access, issue: IssueId, field: FieldId, value: i64) {
    issues::set_field(
        pool,
        who,
        Origin::Web,
        issue,
        field,
        FieldChange::Set(Some(Input::Ids(vec![value]))),
        false,
    )
    .await
    .unwrap();
}

async fn held(pool: &PgPool, issue: IssueId, field: FieldId) -> Vec<i64> {
    sqlx::query_scalar(
        "select value from issue_ref_values where issue_id = $1 and field_id = $2 order by value",
    )
    .bind(issue)
    .bind(field)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn titled(pool: &PgPool, who: &Access, project: ProjectId, summary: &str) -> IssueId {
    issues::create(
        pool,
        who,
        Origin::Web,
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
    .issue
    .id
}

async fn shown(
    pool: &PgPool,
    who: &Access,
    id: BoardId,
    q: Option<&str>,
    now: Option<OffsetDateTime>,
) -> Vec<IssueId> {
    let v = view::view(pool, who, id, None, q, None, now).await.unwrap();
    v.cards.iter().map(|c| c.issue).collect()
}

/// A kanban board of the project in mode `query` with this query.
async fn query_board(pool: &PgPool, who: &Access, p: ProjectId, query: &str) -> BoardRow {
    let k = board(pool, who, Preset::Kanban, &[p]).await;
    change(
        pool,
        who,
        k.full.board.id,
        BoardChange {
            mode: Some(Mode::Query),
            query: Some(query.to_owned()),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn qry_049_query_board_by_its_query() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let kind = field(pool, "Type").await;
    let bug = value(pool, p, kind, "Bug").await;
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    set(pool, &admin, a, kind, bug).await;
    let q = query_board(pool, &admin, p, "type: Bug").await;
    let name = q.full.board.name.clone();
    let found = |text: String| {
        search::search(
            pool,
            &admin,
            SearchInput {
                text,
                limit: 50,
                ..SearchInput::default()
            },
        )
    };
    let ids = |page: search::SearchPage| -> Vec<IssueId> {
        page.issues.issues.iter().map(|v| v.issue.id).collect()
    };
    assert_eq!(
        ids(found(format!("board: \"{name}\"")).await.unwrap()),
        vec![a]
    );
    assert_eq!(
        ids(found(format!("!board: \"{name}\"")).await.unwrap()),
        vec![b]
    );
    // Its default backlog (`!board: <board>`) loads: the issues the query leaves out.
    let backlog = backlog::get(
        pool,
        &admin,
        q.full.board.id,
        BacklogInput {
            limit: 50,
            ..BacklogInput::default()
        },
    )
    .await
    .unwrap();
    let page = backlog.page.expect("a backlog page");
    assert_eq!(ids(page), vec![b]);
    // A board query names neither its own board nor another query board.
    let other = board(pool, &admin, Preset::Kanban, &[p]).await;
    let own = other.full.board.name.clone();
    for text in [format!("board: \"{own}\""), format!("board: \"{name}\"")] {
        let refused = change(
            pool,
            &admin,
            other.full.board.id,
            BoardChange {
                mode: Some(Mode::Query),
                query: Some(text),
                ..BoardChange::default()
            },
        )
        .await;
        assert!(
            matches!(refused, Err(Error::Invalid(_) | Error::InvalidQuery(_))),
            "{refused:?}"
        );
    }
}

#[tokio::test]
async fn brd_028_query_board_cards() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let kind = field(pool, "Type").await;
    let bug = value(pool, p, kind, "Bug").await;
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    set(pool, &admin, a, kind, bug).await;
    let q = query_board(pool, &admin, p, "type: Bug").await;
    let id = q.full.board.id;
    let hidden = q.full.sprints[0].id;
    assert_eq!(q.query.as_deref(), Some("Type: Bug"));
    assert_eq!(shown(pool, &admin, id, None, None).await, vec![a]);
    // Cards leave through the query only.
    assert!(matches!(
        cards::remove(pool, &admin, Origin::Web, id, hidden, a).await,
        Err(Error::Refused(_))
    ));
    // A bad query answers its problems; an empty one shows every issue.
    assert!(matches!(
        change(
            pool,
            &admin,
            id,
            BoardChange {
                query: Some("type: Nonexistent".to_owned()),
                ..BoardChange::default()
            },
        )
        .await,
        Err(Error::InvalidQuery(_))
    ));
    change(
        pool,
        &admin,
        id,
        BoardChange {
            query: Some(String::new()),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let mut all = shown(pool, &admin, id, None, None).await;
    all.sort_unstable();
    assert_eq!(all, vec![a, b]);
    // No sprints on a query board.
    assert!(matches!(
        change(
            pool,
            &admin,
            id,
            BoardChange {
                sprints_on: Some(true),
                ..BoardChange::default()
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn brd_028_plan_preview_changes_nothing() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let kind = field(pool, "Type").await;
    let bug = value(pool, p, kind, "Bug").await;
    let a = issue(pool, &admin, p).await;
    let q = query_board(pool, &admin, p, "type: Bug").await;
    let id = q.full.board.id;
    let hidden = q.full.sprints[0].id;
    let before = held(pool, a, kind).await;
    let preview = cards::add(
        pool,
        &admin,
        Origin::Web,
        id,
        hidden,
        NewCard {
            issue: a,
            preview: true,
            ..NewCard::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(
        preview.changes,
        vec![Step::Set {
            field: kind,
            value: bug
        }]
    );
    assert!(preview.card.is_some());
    assert_eq!(held(pool, a, kind).await, before);
    assert!(shown(pool, &admin, id, None, None).await.is_empty());
    // Confirmed: the field changes, the issue is a card.
    let done = cards::add(
        pool,
        &admin,
        Origin::Web,
        id,
        hidden,
        NewCard {
            issue: a,
            ..NewCard::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(done.changes.len(), 1);
    assert_eq!(held(pool, a, kind).await, vec![bug]);
    assert_eq!(shown(pool, &admin, id, None, None).await, vec![a]);
}

#[tokio::test]
async fn brd_028_plan_refused() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let a = issue(pool, &admin, p).await;
    let q = query_board(pool, &admin, p, "is: resolved type: Bug").await;
    let kind = field(pool, "Type").await;
    let before = held(pool, a, kind).await;
    assert!(matches!(
        cards::add(
            pool,
            &admin,
            Origin::Web,
            q.full.board.id,
            q.full.sprints[0].id,
            NewCard {
                issue: a,
                ..NewCard::default()
            }
        )
        .await,
        Err(Error::Refused(_))
    ));
    assert_eq!(held(pool, a, kind).await, before);
}

#[tokio::test]
async fn brd_028_switch_to_manual_pins() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let kind = field(pool, "Type").await;
    let bug = value(pool, p, kind, "Bug").await;
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    set(pool, &admin, a, kind, bug).await;
    let q = query_board(pool, &admin, p, "type: Bug").await;
    let id = q.full.board.id;
    let hidden = q.full.sprints[0].id;
    assert!(sprints_of(pool, a).await.is_empty());
    let manual = change(
        pool,
        &admin,
        id,
        BoardChange {
            mode: Some(Mode::Manual),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(manual.query, None);
    assert_eq!(sprints_of(pool, a).await, vec![hidden]);
    assert!(sprints_of(pool, b).await.is_empty());
    // The card stays though it no longer matches.
    let task = value(pool, p, kind, "Task").await;
    set(pool, &admin, a, kind, task).await;
    assert_eq!(shown(pool, &admin, id, None, None).await, vec![a]);
}

#[tokio::test]
async fn brd_030_viewer_relative() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let mia = member(pool, &admin, "mia", &[p]).await;
    let leo = member(pool, &admin, "leo", &[p]).await;
    let assignee = field(pool, "Assignee").await;
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    let set_id: i64 = sqlx::query_scalar(
        "select set_id from project_fields where project_id = $1 and field_id = $2",
    )
    .bind(p)
    .bind(assignee)
    .fetch_one(pool)
    .await
    .unwrap();
    taskmaster_service::value_sets::add_members(
        pool,
        &admin,
        set_id,
        &[leo.user(), mia.user()],
        &[],
    )
    .await
    .unwrap();
    set(pool, &admin, a, assignee, leo.user()).await;
    set(pool, &admin, b, assignee, mia.user()).await;
    let k = boards::create(
        pool,
        &admin,
        Language::En,
        NewBoard {
            preset: Preset::Kanban,
            name: "Mine".to_owned(),
            projects: vec![p],
            sharing: Sharing {
                view_readers: true,
                ..Sharing::default()
            },
        },
    )
    .await
    .unwrap();
    let id = k.full.board.id;
    change(
        pool,
        &admin,
        id,
        BoardChange {
            mode: Some(Mode::Query),
            query: Some("assignee: me".to_owned()),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(shown(pool, &leo, id, None, None).await, vec![a]);
    assert_eq!(shown(pool, &mia, id, None, None).await, vec![b]);
}

#[tokio::test]
async fn brd_030_today_moves() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let a = issue(pool, &admin, p).await;
    let q = query_board(pool, &admin, p, "created: today").await;
    let id = q.full.board.id;
    assert_eq!(shown(pool, &admin, id, None, None).await, vec![a]);
    let later = OffsetDateTime::now_utc() + time::Duration::days(3);
    assert!(shown(pool, &admin, id, None, Some(later)).await.is_empty());
}

/// A sprintless manual board with cards in its first column, added in this order.
async fn manual_board(pool: &PgPool, who: &Access, p: ProjectId, cards: &[IssueId]) -> BoardRow {
    let k = board(pool, who, Preset::Kanban, &[p]).await;
    for c in cards {
        add(pool, who, k.full.board.id, k.full.sprints[0].id, *c).await;
    }
    k
}

#[tokio::test]
async fn brd_070_sort_per_cell() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let b = titled(pool, &admin, p, "Beta").await;
    let a = titled(pool, &admin, p, "Alpha").await;
    let c = titled(pool, &admin, p, "Gamma").await;
    let k = manual_board(pool, &admin, p, &[b, a, c]).await;
    let id = k.full.board.id;
    assert_eq!(shown(pool, &admin, id, None, None).await, vec![b, a, c]);
    let v = view::view(
        pool,
        &admin,
        id,
        None,
        Some("sort: summary desc"),
        None,
        None,
    )
    .await
    .unwrap();
    assert!(v.sort.is_some());
    let order: Vec<IssueId> = v.cards.iter().map(|c| c.issue).collect();
    assert_eq!(order, vec![c, b, a]);
    let plain = view::view(pool, &admin, id, None, None, None, None)
        .await
        .unwrap();
    assert!(plain.sort.is_none());
}

#[tokio::test]
async fn brd_070_manual_move_refused() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let a = titled(pool, &admin, p, "Alpha").await;
    let b = titled(pool, &admin, p, "Beta").await;
    let k = manual_board(pool, &admin, p, &[a, b]).await;
    let id = k.full.board.id;
    let hidden = k.full.sprints[0].id;
    let column: ColumnId = k.full.columns[0].id;
    let seen = issues::get(pool, &admin, a).await.unwrap().0.issue.version;
    let to = |header: Option<&str>| Move {
        lane: None,
        column,
        after: Some(b),
        before: None,
        version: seen,
        header: header.map(str::to_owned),
    };
    match cards::move_card(
        pool,
        &admin,
        Origin::Web,
        id,
        hidden,
        a,
        to(Some("sort: summary")),
    )
    .await
    {
        Err(Error::Invalid(fields)) => assert_eq!(fields[0].path, "after"),
        other => panic!("{other:?}"),
    }
    cards::move_card(pool, &admin, Origin::Web, id, hidden, a, to(None))
        .await
        .unwrap();
    assert_eq!(shown(pool, &admin, id, None, None).await, vec![b, a]);
}

#[tokio::test]
async fn brd_081_filter_hides_cards() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let kind = field(pool, "Type").await;
    let bug = value(pool, p, kind, "Bug").await;
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    set(pool, &admin, a, kind, bug).await;
    let k = manual_board(pool, &admin, p, &[a, b]).await;
    let id = k.full.board.id;
    assert_eq!(
        shown(pool, &admin, id, Some("type: bug"), None).await,
        vec![a]
    );
    let v = view::view(pool, &admin, id, None, Some("type: bug"), None, None)
        .await
        .unwrap();
    assert_eq!(v.total, 1);
    // The live board asks with the filter: the other card no longer matches.
    let (found, _) = view::batch(pool, &admin, id, None, &[a, b], Some("type: bug"), None)
        .await
        .unwrap();
    assert!(found[0].is_some() && found[1].is_none());
    assert!(matches!(
        view::view(
            pool,
            &admin,
            id,
            None,
            Some("type: nothing-like-it"),
            None,
            None
        )
        .await,
        Err(Error::InvalidQuery(_))
    ));
}

#[tokio::test]
async fn brd_028_move_ranks_unranked_cards() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    let c = issue(pool, &admin, p).await;
    let q = query_board(pool, &admin, p, "").await;
    let id = q.full.board.id;
    let hidden = q.full.sprints[0].id;
    // Unranked cards show by issue ID.
    assert_eq!(shown(pool, &admin, id, None, None).await, vec![a, b, c]);
    let seen = issues::get(pool, &admin, a).await.unwrap().0.issue.version;
    cards::move_card(
        pool,
        &admin,
        Origin::Web,
        id,
        hidden,
        a,
        Move {
            lane: None,
            column: q.full.columns[0].id,
            after: Some(b),
            before: None,
            version: seen,
            header: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(shown(pool, &admin, id, None, None).await, vec![b, a, c]);
}

/// How many issue events of this issue name board `on`.
async fn issue_events(pool: &PgPool, issue: IssueId, on: BoardId) -> i64 {
    sqlx::query_scalar(
        "select count(*) from live_events where kind = 'issue' and entity_id = $1
         and $2 = any(boards)",
    )
    .bind(issue)
    .bind(on)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn brd_114_query_board_hears_named_sprint_go() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let a = issue(pool, &admin, p).await;
    let x = board(pool, &admin, Preset::Scrum, &[p]).await;
    let xid = x.full.board.id;
    change(
        pool,
        &admin,
        xid,
        BoardChange {
            name: Some("Sprints".to_owned()),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let s2 = sprint(pool, &admin, xid, named("Second")).await;
    add(pool, &admin, xid, s2, a).await;
    let y = query_board(pool, &admin, p, "board: Sprints").await;
    assert_eq!(
        shown(pool, &admin, y.full.board.id, None, None).await,
        vec![a]
    );
    let before = issue_events(pool, a, xid).await;
    sprints::delete(pool, &admin, xid, s2).await.unwrap();
    // The card's event reaches every board of its project, the query board among them.
    assert_eq!(issue_events(pool, a, xid).await, before + 1);
    assert!(
        shown(pool, &admin, y.full.board.id, None, None)
            .await
            .is_empty()
    );
}
