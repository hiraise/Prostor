//! Backlogs, template boards and boards from the issue's side (step 1.8b-4, D-045).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use time::OffsetDateTime;
use time::macros::datetime;
use trackflow_domain::access::Access;
use trackflow_domain::access::role::PredefinedRole;
use trackflow_domain::boards::{Mode, Preset};
use trackflow_domain::issues::history::Origin;
use trackflow_domain::projects::Template;
use trackflow_domain::settings::Language;
use trackflow_domain::{BoardId, IssueId, ProjectId};
use trackflow_service::boards::backlog::{self, BacklogInput, Missing};
use trackflow_service::boards::issue_side;
use trackflow_service::boards::{self as boards, BoardChange, BoardRow, NewBoard, Sharing};
use trackflow_service::issues::{self, AudienceInput, NewIssue};
use trackflow_service::projects::{self, NewProjectInput};
use trackflow_service::setup::{self, Setup};
use trackflow_service::users::{self, NewAccount};
use trackflow_service::{Error, Passwords, access, roles, saved_searches};
use trackflow_store::PgPool;
use trackflow_store::access::Holder;
use trackflow_store::sessions::Origin as SessionOrigin;
use trackflow_testkit::TestDb;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);

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

async fn project(pool: &PgPool, actor: &Access, key: &str, template: Template) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template,
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

/// An account, a member of the projects.
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
    if !of.is_empty() {
        let role = trackflow_store::roles::builtin_id(pool, PredefinedRole::ProjectMember.key())
            .await
            .unwrap()
            .unwrap();
        let scopes: Vec<Option<ProjectId>> = of.iter().map(|p| Some(*p)).collect();
        roles::assign(pool, admin, role, Holder::User(user), &scopes)
            .await
            .unwrap();
    }
    access::load(pool, user).await.unwrap()
}

async fn issue(pool: &PgPool, actor: &Access, project: ProjectId) -> IssueId {
    issue_for(pool, actor, project, None).await
}

async fn issue_for(
    pool: &PgPool,
    actor: &Access,
    project: ProjectId,
    audience: Option<AudienceInput>,
) -> IssueId {
    issues::create(
        pool,
        actor,
        Origin::Web,
        NewIssue {
            project,
            summary: "Something".to_owned(),
            publish: true,
            audience,
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

async fn create(pool: &PgPool, who: &Access, preset: Preset, projects: &[ProjectId]) -> BoardRow {
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

async fn change(
    pool: &PgPool,
    who: &Access,
    b: BoardId,
    c: BoardChange,
) -> Result<BoardRow, Error> {
    let now = boards::get(pool, who, b).await?;
    boards::update(
        pool,
        who,
        Language::En,
        b,
        BoardChange {
            version: now.full.board.version,
            ..c
        },
    )
    .await
}

async fn draft(pool: &PgPool, actor: &Access, project: ProjectId) -> IssueId {
    issues::create(
        pool,
        actor,
        Origin::Web,
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

/// Every issue of the backlog, page by page.
async fn backlog_ids(pool: &PgPool, who: &Access, board: BoardId, limit: u32) -> Vec<IssueId> {
    let mut all = Vec::new();
    let mut cursor = None;
    loop {
        let b = backlog::get(
            pool,
            who,
            board,
            BacklogInput {
                cursor,
                limit,
                ..BacklogInput::default()
            },
        )
        .await
        .unwrap();
        let page = b.page.expect("a backlog page");
        all.extend(page.issues.issues.iter().map(|v| v.issue.id));
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => return all,
        }
    }
}

fn blank_change() -> BoardChange {
    BoardChange {
        name: None,
        projects: None,
        owner: None,
        sharing: None,
        column_field: None,
        sprints_on: None,
        mode: None,
        query: None,
        auto_sprint: None,
        backlog: None,
        new_backlog: false,
        lanes: None,
        cards: None,
        version: 0,
    }
}

#[tokio::test]
async fn brd_089_default_backlog() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "APP", Template::Blank).await;
    let board = create(pool, &admin, Preset::Kanban, &[p]).await;
    let search = board.full.board.backlog_search_id.expect("a backlog");
    let saved = saved_searches::get(pool, &admin, search).await.unwrap();
    assert_eq!(saved.row.name, "Team · backlog");
    assert_eq!(saved.text, "project: APP is: unresolved !board: Team");
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    let shown = backlog_ids(pool, &admin, board.full.board.id, 50).await;
    assert_eq!(shown.len(), 2);
    // BRD-096: an issue sent to the board leaves the backlog.
    let sprint = board.full.sprints[0].id;
    trackflow_service::boards::cards::add(
        pool,
        &admin,
        Origin::Web,
        board.full.board.id,
        sprint,
        trackflow_service::boards::cards::NewCard {
            issue: a,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        backlog_ids(pool, &admin, board.full.board.id, 50).await,
        vec![b]
    );
    // A second board of the same name gets a free search name.
    let again = create(pool, &admin, Preset::Kanban, &[p]).await;
    let other = again.full.board.backlog_search_id.unwrap();
    let saved = saved_searches::get(pool, &admin, other).await.unwrap();
    assert_eq!(saved.row.name, "Team · backlog 2");
    // Someone who does not view the search sees why there is no page.
    let outsider = member(pool, &admin, "olga", &[p]).await;
    let shared = change(
        pool,
        &admin,
        board.full.board.id,
        BoardChange {
            sharing: Some(Sharing {
                view: trackflow_domain::sharing::Audience {
                    users: vec![outsider.user()],
                    groups: Vec::new(),
                },
                ..Sharing::default()
            }),
            ..blank_change()
        },
    )
    .await;
    if shared.is_ok() {
        let b = backlog::get(
            pool,
            &outsider,
            board.full.board.id,
            BacklogInput {
                limit: 10,
                ..BacklogInput::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(b.missing, Some(Missing::Hidden));
    }
}

#[tokio::test]
async fn brd_090_switch_checks_projects() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "APP", Template::Blank).await;
    project(pool, &admin, "WEB", Template::Blank).await;
    let board = create(pool, &admin, Preset::Kanban, &[p]).await;
    let id = board.full.board.id;
    for text in ["project: WEB", "is: unresolved", "project: APP, WEB"] {
        let other = saved_searches::create(pool, &admin, text, text)
            .await
            .unwrap();
        let refused = change(
            pool,
            &admin,
            id,
            BoardChange {
                backlog: Some(Some(other.row.id)),
                ..blank_change()
            },
        )
        .await;
        assert!(
            matches!(&refused, Err(Error::Invalid(p)) if p.iter().any(|f| f.path == "backlog")),
            "{text}: {refused:?}"
        );
    }
    let mine = saved_searches::create(pool, &admin, "Mine", "project: APP assignee: me")
        .await
        .unwrap();
    let now = change(
        pool,
        &admin,
        id,
        BoardChange {
            backlog: Some(Some(mine.row.id)),
            ..blank_change()
        },
    )
    .await
    .unwrap();
    assert_eq!(now.full.board.backlog_search_id, Some(mine.row.id));
    let none = change(
        pool,
        &admin,
        id,
        BoardChange {
            backlog: Some(None),
            ..blank_change()
        },
    )
    .await
    .unwrap();
    assert_eq!(none.full.board.backlog_search_id, None);
    let b = backlog::get(
        pool,
        &admin,
        id,
        BacklogInput {
            limit: 10,
            ..BacklogInput::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(b.missing, Some(Missing::None));
    // A new default backlog for a board without one, owned by whoever asks.
    let made = change(
        pool,
        &admin,
        id,
        BoardChange {
            new_backlog: true,
            ..blank_change()
        },
    )
    .await
    .unwrap();
    let search = made.full.board.backlog_search_id.expect("a new backlog");
    let saved = saved_searches::get(pool, &admin, search).await.unwrap();
    assert_eq!(saved.row.name, "Team · backlog 2");
    // An editor of the board who is not one of the search changes nothing.
    let editor = member(pool, &admin, "eve", &[p]).await;
    let refused = change(
        pool,
        &editor,
        id,
        BoardChange {
            backlog: Some(Some(mine.row.id)),
            ..blank_change()
        },
    )
    .await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn brd_093_order_ranked_first_and_cursor_phases() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "APP", Template::Blank).await;
    let board = create(pool, &admin, Preset::Kanban, &[p]).await;
    let id = board.full.board.id;
    let mut made = Vec::new();
    for _ in 0..4 {
        made.push(issue(pool, &admin, p).await);
    }
    let base = backlog_ids(pool, &admin, id, 50).await;
    assert_eq!(base.len(), 4);
    let (w, x, y, z) = (base[0], base[1], base[2], base[3]);
    // The last one first: ranked issues come before the rest.
    backlog::order(pool, &admin, id, z, None, None)
        .await
        .unwrap();
    assert_eq!(backlog_ids(pool, &admin, id, 50).await, vec![z, w, x, y]);
    // BRD-116: the board hears of it.
    let (boards_, patch): (Vec<i64>, serde_json::Value) = sqlx::query_as(
        "select boards, patch from live_events where kind = 'card_order' order by id desc limit 1",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(boards_, vec![id]);
    assert_eq!(patch["search"], board.full.board.backlog_search_id.unwrap());
    // Before a ranked neighbour.
    backlog::order(pool, &admin, id, y, None, Some(z))
        .await
        .unwrap();
    assert_eq!(backlog_ids(pool, &admin, id, 50).await, vec![y, z, w, x]);
    // After an unranked neighbour: the unranked ones above it get ranks in their order.
    backlog::order(pool, &admin, id, y, Some(x), None)
        .await
        .unwrap();
    assert_eq!(backlog_ids(pool, &admin, id, 50).await, vec![z, w, x, y]);
    // Pages of one cross from the ranked phase into the rest.
    backlog::discard(pool, &admin, id).await.unwrap();
    backlog::order(pool, &admin, id, y, None, None)
        .await
        .unwrap();
    assert_eq!(backlog_ids(pool, &admin, id, 1).await, vec![y, w, x, z]);
    assert_eq!(backlog_ids(pool, &admin, id, 2).await, vec![y, w, x, z]);
    // A cursor of the plain search is not one of the backlog.
    backlog::discard(pool, &admin, id).await.unwrap();
    assert_eq!(backlog_ids(pool, &admin, id, 50).await, base);
}

#[tokio::test]
async fn brd_093_order_rights() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "APP", Template::Blank).await;
    let board = create(pool, &admin, Preset::Kanban, &[p]).await;
    let id = board.full.board.id;
    let a = issue(pool, &admin, p).await;
    let other = member(pool, &admin, "bob", &[p]).await;
    // Bob views the board and the search but edits neither.
    change(
        pool,
        &admin,
        id,
        BoardChange {
            sharing: Some(Sharing {
                view: trackflow_domain::sharing::Audience {
                    users: vec![other.user()],
                    groups: Vec::new(),
                },
                ..Sharing::default()
            }),
            ..blank_change()
        },
    )
    .await
    .unwrap();
    let refused = backlog::order(pool, &other, id, a, None, None).await;
    assert!(matches!(refused, Err(Error::Refused(_))), "{refused:?}");
    let refused = backlog::discard(pool, &other, id).await;
    assert!(matches!(refused, Err(Error::Refused(_))), "{refused:?}");
    let missing = backlog::order(pool, &admin, id, a, Some(IssueId::MAX), None).await;
    assert!(missing.is_err());
}

#[tokio::test]
async fn brd_033_issue_boards() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "APP", Template::Blank).await;
    let scrum = create(pool, &admin, Preset::Scrum, &[p]).await;
    let kanban = create(pool, &admin, Preset::Kanban, &[p]).await;
    let a = issue(pool, &admin, p).await;
    let now = T0;
    let shown = issue_side::add(pool, &admin, Origin::Web, a, scrum.full.board.id, None, now)
        .await
        .unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0].sprint.as_ref().map(|s| s.1.as_str()),
        Some("Sprint 1")
    );
    let shown = issue_side::add(
        pool,
        &admin,
        Origin::Web,
        a,
        kanban.full.board.id,
        None,
        now,
    )
    .await
    .unwrap();
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[1].sprint, None);
    // A reader of the project who does not view the boards sees none of them.
    let reader = member(pool, &admin, "rita", &[p]).await;
    assert!(
        issue_side::of_issue(pool, &reader, a)
            .await
            .unwrap()
            .is_empty()
    );
    let page = issues::page::page(
        pool,
        &admin,
        a,
        issues::activity::Query {
            comments: true,
            changes: true,
            from: None,
            direction: trackflow_store::issue_history::Direction::Older,
            limit: 10,
            around_comment: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.boards.len(), 2);
    let shown = issue_side::remove(pool, &admin, Origin::Web, a, scrum.full.board.id)
        .await
        .unwrap();
    assert_eq!(shown.len(), 1);
    let gone = issue_side::remove(pool, &admin, Origin::Web, a, scrum.full.board.id).await;
    assert!(matches!(gone, Err(Error::NotFound(_))), "{gone:?}");
}

#[tokio::test]
async fn iss_007_board_preview_and_skip() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "APP", Template::Blank).await;
    let auto = create(pool, &admin, Preset::Kanban, &[p]).await;
    let auto_id = auto.full.board.id;
    change(
        pool,
        &admin,
        auto_id,
        BoardChange {
            mode: Some(Mode::Auto),
            ..blank_change()
        },
    )
    .await
    .unwrap();
    let manual = create(pool, &admin, Preset::Scrum, &[p]).await;
    // A draft previews the auto board.
    let kept = draft(pool, &admin, p).await;
    let shown = issue_side::of_issue(pool, &admin, kept).await.unwrap();
    assert_eq!(shown.len(), 1);
    assert!(shown[0].auto && !shown[0].skipped);
    // Kept off it, and put on the manual board by hand.
    let shown = issue_side::remove(pool, &admin, Origin::Web, kept, auto_id)
        .await
        .unwrap();
    assert!(shown[0].skipped);
    issue_side::add(
        pool,
        &admin,
        Origin::Web,
        kept,
        manual.full.board.id,
        None,
        T0,
    )
    .await
    .unwrap();
    issues::publish(pool, &admin, Origin::Web, kept)
        .await
        .unwrap();
    let shown = issue_side::of_issue(pool, &admin, kept).await.unwrap();
    let ids: Vec<BoardId> = shown.iter().map(|b| b.board).collect();
    assert_eq!(ids, vec![manual.full.board.id]);
    // Kept off and let back on: it joins.
    let back = draft(pool, &admin, p).await;
    issue_side::remove(pool, &admin, Origin::Web, back, auto_id)
        .await
        .unwrap();
    let shown = issue_side::add(pool, &admin, Origin::Web, back, auto_id, None, T0)
        .await
        .unwrap();
    assert!(shown[0].auto && !shown[0].skipped);
    issues::publish(pool, &admin, Origin::Web, back)
        .await
        .unwrap();
    let shown = issue_side::of_issue(pool, &admin, back).await.unwrap();
    assert_eq!(shown.len(), 1);
    assert!(!shown[0].auto && shown[0].board == auto_id);
}

async fn template_board(pool: &PgPool, admin: &Access, p: ProjectId) -> BoardRow {
    let list = boards::list(pool, admin, None, Some(p), None, 10)
        .await
        .unwrap();
    assert_eq!(list.items.len(), 1);
    boards::get(pool, admin, list.items[0].id).await.unwrap()
}

#[tokio::test]
async fn prj_023_scrum_board() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let board = template_board(pool, &admin, p).await;
    let b = &board.full.board;
    assert_eq!(b.name, "Project SCR");
    assert_eq!(b.preset, "scrum");
    assert!(b.sprints_on);
    assert_eq!(
        board.column_field.as_ref().map(|f| f.name.as_str()),
        Some("State")
    );
    assert!(!board.full.columns.is_empty());
    assert_eq!(b.view_groups.len(), 1);
    let search = saved_searches::get(pool, &admin, b.backlog_search_id.unwrap())
        .await
        .unwrap();
    assert_eq!(search.row.name, "Project SCR · backlog");
    assert_eq!(search.row.view_groups, b.view_groups);
    // A member of the team sees the board.
    let m = member(pool, &admin, "tom", &[]).await;
    trackflow_service::teams::add(pool, &admin, p, &[m.user()], &[])
        .await
        .unwrap();
    let m = access::load(pool, m.user()).await.unwrap();
    assert!(boards::get(pool, &m, b.id).await.is_ok());
}

#[tokio::test]
async fn prj_024_kanban_board() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "KAN", Template::Kanban).await;
    let board = template_board(pool, &admin, p).await;
    let b = &board.full.board;
    assert_eq!(b.preset, "kanban");
    assert!(!b.sprints_on);
    assert_eq!(
        board.column_field.as_ref().map(|f| f.name.as_str()),
        Some("Stage")
    );
    assert_eq!(board.full.columns.len(), 6);
    assert!(b.backlog_search_id.is_some());
    // A blank project gets no board.
    let blank = project(pool, &admin, "BLA", Template::Blank).await;
    let list = boards::list(pool, &admin, None, Some(blank), None, 10)
        .await
        .unwrap();
    assert!(list.items.is_empty());
}
