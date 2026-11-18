//! Lanes (BRD-049…BRD-057, BRD-096; plan step 1.8 C1) on a real database: settings, value and
//! parent lanes, the other lane, moves across lanes, parent lanes made, ordered and sent back to
//! the backlog, and pages of lanes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prostor_domain::access::Access;
use prostor_domain::boards::Preset;
use prostor_domain::boards::lanes::{Attribute, LaneMode, OtherLane};
use prostor_domain::issues::history::Origin;
use prostor_domain::issues::values::Input;
use prostor_domain::links::Side;
use prostor_domain::projects::Template;
use prostor_domain::settings::Language;
use prostor_domain::{BoardId, FieldId, IssueId, ProjectId, SprintId};
use prostor_service::boards::cards::{self, Move, NewCard};
use prostor_service::boards::lanes::{self, LanesChange};
use prostor_service::boards::view::{self, BoardView};
use prostor_service::boards::{self as boards, BoardChange, BoardRow, NewBoard, Sharing};
use prostor_service::issue_links::{self, NewLinks};
use prostor_service::issues::{self, FieldChange, NewIssue};
use prostor_service::projects::{self, NewProjectInput};
use prostor_service::setup::{self, Setup};
use prostor_service::{Error, Passwords, access};
use prostor_store::PgPool;
use prostor_store::sessions::Origin as SessionOrigin;
use prostor_testkit::TestDb;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);

async fn admin(pool: &PgPool) -> Access {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    let signed_in = setup::run(
        pool,
        &Passwords::new(4),
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

async fn titled(pool: &PgPool, actor: &Access, project: ProjectId, summary: &str) -> IssueId {
    issues::create(
        pool,
        actor,
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

async fn set(pool: &PgPool, who: &Access, issue: IssueId, field: FieldId, value: Option<i64>) {
    issues::set_field(
        pool,
        who,
        Origin::Web,
        issue,
        field,
        FieldChange::Set(value.map(|v| Input::Ids(vec![v]))),
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

async fn parent_of(pool: &PgPool, child: IssueId) -> Option<IssueId> {
    prostor_store::links::parent_of(pool, child)
        .await
        .unwrap()
        .map(|l| l.id)
}

async fn subtask(pool: &PgPool, who: &Access, parent: IssueId, child: IssueId) {
    let link_type = prostor_store::links::subtask_type(pool)
        .await
        .unwrap()
        .unwrap();
    issue_links::add(
        pool,
        who,
        Origin::Web,
        parent,
        NewLinks {
            link_type,
            side: Side::Source,
            issues: vec![child],
        },
        false,
    )
    .await
    .unwrap();
}

/// A kanban board of the project (manual mode) with these cards.
async fn board(pool: &PgPool, who: &Access, p: ProjectId, cards: &[IssueId]) -> BoardRow {
    let k = boards::create(
        pool,
        who,
        Language::En,
        NewBoard {
            preset: Preset::Kanban,
            name: "Team".to_owned(),
            projects: vec![p],
            sharing: Sharing::default(),
        },
    )
    .await
    .unwrap();
    for c in cards {
        add(pool, who, &k, *c, NewCard::default()).await.unwrap();
    }
    k
}

fn sprint(k: &BoardRow) -> SprintId {
    k.full.sprints[0].id
}

async fn add(
    pool: &PgPool,
    who: &Access,
    k: &BoardRow,
    issue: IssueId,
    card: NewCard,
) -> Result<cards::Placed, Error> {
    cards::add(
        pool,
        who,
        Origin::Web,
        k.full.board.id,
        sprint(k),
        NewCard { issue, ..card },
    )
    .await
    .map(|answer| answer.done)
}

async fn set_lanes(
    pool: &PgPool,
    who: &Access,
    id: BoardId,
    lanes: LanesChange,
) -> Result<BoardRow, Error> {
    let now = boards::get(pool, who, id).await.unwrap();
    boards::update(
        pool,
        who,
        Language::En,
        id,
        BoardChange {
            lanes: Some(lanes),
            version: now.full.board.version,
            ..BoardChange::default()
        },
    )
    .await
}

fn by_values(field: Option<FieldId>, values: Vec<String>) -> LanesChange {
    LanesChange {
        mode: LaneMode::Values,
        field,
        attribute: None,
        values,
        other: OtherLane::Bottom,
        new_card_value: None,
    }
}

fn by_parents() -> LanesChange {
    LanesChange {
        mode: LaneMode::Parents,
        field: None,
        attribute: None,
        values: Vec::new(),
        other: OtherLane::Bottom,
        new_card_value: None,
    }
}

async fn show(pool: &PgPool, who: &Access, id: BoardId, lanes: Option<&str>) -> BoardView {
    view::view(pool, who, id, None, None, lanes, None)
        .await
        .unwrap()
}

fn keys(v: &BoardView) -> Vec<String> {
    v.lanes.iter().map(|l| l.key.clone()).collect()
}

fn lane_of(v: &BoardView, issue: IssueId) -> Option<String> {
    v.cards
        .iter()
        .find(|c| c.issue == issue)
        .and_then(|c| c.lane.clone())
}

async fn to_lane(
    pool: &PgPool,
    who: &Access,
    k: &BoardRow,
    issue: IssueId,
    lane: &str,
) -> Result<cards::Placed, Error> {
    let v = show(pool, who, k.full.board.id, None).await;
    let column = v.cards.iter().find(|c| c.issue == issue).unwrap().column;
    let version = issues::get(pool, who, issue).await.unwrap().0.issue.version;
    cards::move_card(
        pool,
        who,
        Origin::Web,
        k.full.board.id,
        sprint(k),
        issue,
        Move {
            column,
            after: None,
            before: None,
            lane: Some(lane.to_owned()),
            version,
            header: None,
        },
    )
    .await
    .map(|answer| answer.done)
}

fn invalid_at(result: Result<BoardRow, Error>, path: &str) {
    match result {
        Err(Error::Invalid(fields)) => assert_eq!(fields[0].path, path),
        other => panic!("{:?}", other.map(|b| b.full.board.lane_mode)),
    }
}

#[tokio::test]
async fn brd_049_lane_settings() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let k = board(pool, &admin, p, &[]).await;
    let id = k.full.board.id;
    let priority = field(pool, "Priority").await;
    let high = value(pool, p, priority, "High").await;
    let row = set_lanes(
        pool,
        &admin,
        id,
        by_values(Some(priority), vec![format!("v:{high}")]),
    )
    .await
    .unwrap();
    assert_eq!(row.full.board.lane_mode, "values");
    assert_eq!(row.full.board.lane_values, vec![format!("v:{high}")]);
    assert_eq!(row.lane_field.as_ref().unwrap().id, priority);
    // The column field makes no lanes; a value of another field is refused.
    let state = k.full.board.column_field_id;
    invalid_at(
        set_lanes(pool, &admin, id, by_values(state, Vec::new())).await,
        "lanes.field",
    );
    let kind = field(pool, "Type").await;
    let bug = value(pool, p, kind, "Bug").await;
    invalid_at(
        set_lanes(
            pool,
            &admin,
            id,
            by_values(Some(priority), vec![format!("v:{bug}")]),
        )
        .await,
        "lanes.values",
    );
    invalid_at(
        set_lanes(
            pool,
            &admin,
            id,
            by_values(Some(priority), vec!["u:1".into()]),
        )
        .await,
        "lanes.values",
    );
    invalid_at(
        set_lanes(pool, &admin, id, by_values(None, Vec::new())).await,
        "lanes.field",
    );
    // Parents are limited by a field with a list of values, never a user field.
    let assignee = field(pool, "Assignee").await;
    invalid_at(
        set_lanes(
            pool,
            &admin,
            id,
            LanesChange {
                field: Some(assignee),
                ..by_parents()
            },
        )
        .await,
        "lanes.field",
    );
    // A value for new cards only with parent lanes (BRD-055).
    invalid_at(
        set_lanes(
            pool,
            &admin,
            id,
            LanesChange {
                new_card_value: Some(bug),
                ..by_values(Some(priority), Vec::new())
            },
        )
        .await,
        "lanes.new_card_value",
    );
    let row = set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            new_card_value: Some(bug),
            ..by_parents()
        },
    )
    .await
    .unwrap();
    assert_eq!(row.full.board.new_card_value_id, Some(bug));
    // Lanes off: everything is cleared.
    let row = set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            mode: LaneMode::None,
            ..by_parents()
        },
    )
    .await
    .unwrap();
    assert_eq!(row.full.board.lane_mode, "none");
    assert_eq!(row.full.board.new_card_value_id, None);
    assert!(show(pool, &admin, id, None).await.lanes.is_empty());
}

#[tokio::test]
async fn brd_050_value_lanes_and_moves() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let priority = field(pool, "Priority").await;
    let high = value(pool, p, priority, "High").await;
    let normal = value(pool, p, priority, "Normal").await;
    let low = value(pool, p, priority, "Low").await;
    let a = titled(pool, &admin, p, "A").await;
    let b = titled(pool, &admin, p, "B").await;
    let c = titled(pool, &admin, p, "C").await;
    set(pool, &admin, a, priority, Some(high)).await;
    set(pool, &admin, b, priority, Some(normal)).await;
    set(pool, &admin, c, priority, Some(low)).await;
    let k = board(pool, &admin, p, &[a, b, c]).await;
    let id = k.full.board.id;
    let (h, n) = (format!("v:{high}"), format!("v:{normal}"));
    set_lanes(
        pool,
        &admin,
        id,
        by_values(Some(priority), vec![h.clone(), n.clone()]),
    )
    .await
    .unwrap();
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec![h.clone(), n.clone(), "other".to_owned()]);
    assert_eq!(lane_of(&v, a), Some(h.clone()));
    assert_eq!(lane_of(&v, b), Some(n.clone()));
    assert_eq!(lane_of(&v, c), Some("other".to_owned()));
    assert_eq!(
        v.lanes.iter().map(|l| l.count).collect::<Vec<_>>(),
        vec![1, 1, 1]
    );
    assert_eq!(v.total, 3);
    // Dropping into a lane sets its value.
    let placed = to_lane(pool, &admin, &k, c, &h).await.unwrap();
    assert_eq!(placed.card.unwrap().lane, Some(h.clone()));
    assert_eq!(held(pool, c, priority).await, vec![high]);
    // Lanes by account: `other` clears the field.
    let assignee = field(pool, "Assignee").await;
    let me = format!("u:{}", admin.user());
    set(pool, &admin, a, assignee, Some(admin.user())).await;
    set_lanes(
        pool,
        &admin,
        id,
        by_values(Some(assignee), vec![me.clone()]),
    )
    .await
    .unwrap();
    let v = show(pool, &admin, id, None).await;
    assert_eq!(lane_of(&v, a), Some(me.clone()));
    assert_eq!(lane_of(&v, b), Some("other".to_owned()));
    to_lane(pool, &admin, &k, b, &me).await.unwrap();
    assert_eq!(held(pool, b, assignee).await, vec![admin.user()]);
    to_lane(pool, &admin, &k, a, "other").await.unwrap();
    assert!(held(pool, a, assignee).await.is_empty());
    // An unknown lane is refused.
    assert!(matches!(
        to_lane(pool, &admin, &k, a, "u:999999").await,
        Err(Error::Invalid(_))
    ));
    // Project lanes show where a card stands; a drop does not move the issue.
    set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            attribute: Some(Attribute::Project),
            values: vec![format!("p:{p}")],
            ..by_values(None, Vec::new())
        },
    )
    .await
    .unwrap();
    let v = show(pool, &admin, id, None).await;
    assert_eq!(lane_of(&v, a), Some(format!("p:{p}")));
    assert!(matches!(
        to_lane(pool, &admin, &k, a, "other").await,
        Err(Error::Refused(_))
    ));
}

#[tokio::test]
async fn brd_050_date_lanes() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let a = titled(pool, &admin, p, "A").await;
    let k = board(pool, &admin, p, &[a]).await;
    let id = k.full.board.id;
    set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            attribute: Some(Attribute::Created),
            ..by_values(None, Vec::new())
        },
    )
    .await
    .unwrap();
    let v = show(pool, &admin, id, None).await;
    // Every range, then `other`.
    assert_eq!(v.lanes_total, 9);
    assert_eq!(lane_of(&v, a), Some("d:today".to_owned()));
    // Only some ranges: a card in none of them goes to `other`.
    set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            attribute: Some(Attribute::Created),
            values: vec!["d:older".into()],
            ..by_values(None, Vec::new())
        },
    )
    .await
    .unwrap();
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec!["d:older".to_owned(), "other".to_owned()]);
    assert_eq!(lane_of(&v, a), Some("other".to_owned()));
}

#[tokio::test]
async fn brd_052_other_lane() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let priority = field(pool, "Priority").await;
    let high = value(pool, p, priority, "High").await;
    let low = value(pool, p, priority, "Low").await;
    let a = titled(pool, &admin, p, "A").await;
    let b = titled(pool, &admin, p, "B").await;
    set(pool, &admin, a, priority, Some(high)).await;
    set(pool, &admin, b, priority, Some(low)).await;
    let k = board(pool, &admin, p, &[a, b]).await;
    let id = k.full.board.id;
    let h = format!("v:{high}");
    set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            other: OtherLane::Top,
            ..by_values(Some(priority), vec![h.clone()])
        },
    )
    .await
    .unwrap();
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec!["other".to_owned(), h.clone()]);
    set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            other: OtherLane::Off,
            ..by_values(Some(priority), vec![h.clone()])
        },
    )
    .await
    .unwrap();
    // Cards matching no lane are not on the board then.
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec![h]);
    assert_eq!(v.cards.iter().map(|c| c.issue).collect::<Vec<_>>(), vec![a]);
    assert_eq!(v.total, 1);
}

#[tokio::test]
async fn brd_051_parent_lanes_and_reparenting() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let epic = titled(pool, &admin, p, "Epic").await;
    let a = titled(pool, &admin, p, "A").await;
    let b = titled(pool, &admin, p, "B").await;
    let c = titled(pool, &admin, p, "C").await;
    subtask(pool, &admin, epic, a).await;
    subtask(pool, &admin, epic, b).await;
    let k = board(pool, &admin, p, &[a, b, c]).await;
    let id = k.full.board.id;
    set_lanes(pool, &admin, id, by_parents()).await.unwrap();
    let v = show(pool, &admin, id, None).await;
    let e = format!("i:{epic}");
    assert_eq!(keys(&v), vec![e.clone(), "other".to_owned()]);
    assert_eq!(v.lanes[0].issue, Some(epic));
    assert_eq!(v.lanes[0].count, 2);
    // The parent's issue comes along for the header.
    assert!(v.issues.issues.iter().any(|i| i.issue.id == epic));
    assert_eq!(lane_of(&v, a), Some(e.clone()));
    assert_eq!(lane_of(&v, c), Some("other".to_owned()));
    // Another lane re-parents; `other` removes the parent.
    to_lane(pool, &admin, &k, c, &e).await.unwrap();
    assert_eq!(parent_of(pool, c).await, Some(epic));
    to_lane(pool, &admin, &k, a, "other").await.unwrap();
    assert_eq!(parent_of(pool, a).await, None);
    let v = show(pool, &admin, id, None).await;
    assert_eq!(lane_of(&v, a), Some("other".to_owned()));
    assert_eq!(lane_of(&v, c), Some(e.clone()));
    // A field limits the parents: an issue without the value is no lane.
    let kind = field(pool, "Type").await;
    let epic_type = value(pool, p, kind, "Epic").await;
    set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            field: Some(kind),
            values: vec![format!("v:{epic_type}")],
            ..by_parents()
        },
    )
    .await
    .unwrap();
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec!["other".to_owned()]);
    set(pool, &admin, epic, kind, Some(epic_type)).await;
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec![e, "other".to_owned()]);
}

#[tokio::test]
async fn brd_096_parent_becomes_lane() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let story = titled(pool, &admin, p, "Story").await;
    let a = titled(pool, &admin, p, "A").await;
    let b = titled(pool, &admin, p, "B").await;
    let lone = titled(pool, &admin, p, "Lone").await;
    subtask(pool, &admin, story, a).await;
    subtask(pool, &admin, story, b).await;
    let k = board(pool, &admin, p, &[]).await;
    let id = k.full.board.id;
    set_lanes(pool, &admin, id, by_parents()).await.unwrap();
    // A parent joins as a lane with its subtasks; an issue without subtasks is a card.
    let placed = add(
        pool,
        &admin,
        &k,
        story,
        NewCard {
            subtasks: true,
            ..NewCard::default()
        },
    )
    .await
    .unwrap();
    assert!(placed.card.is_none());
    add(pool, &admin, &k, lone, NewCard::default())
        .await
        .unwrap();
    let v = show(pool, &admin, id, None).await;
    let s = format!("i:{story}");
    assert_eq!(keys(&v), vec![s.clone(), "other".to_owned()]);
    assert_eq!(lane_of(&v, a), Some(s.clone()));
    assert_eq!(lane_of(&v, b), Some(s.clone()));
    assert_eq!(lane_of(&v, lone), Some("other".to_owned()));
    assert!(v.cards.iter().all(|c| c.issue != story));
    // Dropped into a lane, a card gets that parent.
    let late = titled(pool, &admin, p, "Late").await;
    add(
        pool,
        &admin,
        &k,
        late,
        NewCard {
            lane: Some(s.clone()),
            ..NewCard::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(parent_of(pool, late).await, Some(story));
}

#[tokio::test]
async fn brd_053_lane_order() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let one = titled(pool, &admin, p, "One").await;
    let two = titled(pool, &admin, p, "Two").await;
    let a = titled(pool, &admin, p, "A").await;
    let b = titled(pool, &admin, p, "B").await;
    subtask(pool, &admin, one, a).await;
    subtask(pool, &admin, two, b).await;
    let k = board(pool, &admin, p, &[a, b]).await;
    let id = k.full.board.id;
    set_lanes(pool, &admin, id, by_parents()).await.unwrap();
    let (l1, l2) = (format!("i:{one}"), format!("i:{two}"));
    // Lanes without a rank follow their first card.
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec![l1.clone(), l2.clone(), "other".to_owned()]);
    lanes::move_lane(pool, &admin, id, sprint(&k), one, Some(two), None)
        .await
        .unwrap();
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec![l2.clone(), l1.clone(), "other".to_owned()]);
    lanes::move_lane(pool, &admin, id, sprint(&k), one, None, Some(two))
        .await
        .unwrap();
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec![l1, l2, "other".to_owned()]);
    // The cards stay cards: a lane rank only orders lanes.
    assert_eq!(v.cards.len(), 2);
    // Value lanes have no lane ranks: they reorder in the settings.
    let priority = field(pool, "Priority").await;
    set_lanes(pool, &admin, id, by_values(Some(priority), Vec::new()))
        .await
        .unwrap();
    assert!(matches!(
        lanes::move_lane(pool, &admin, id, sprint(&k), one, None, None).await,
        Err(Error::Refused(_))
    ));
}

#[tokio::test]
async fn brd_054_new_lane() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let k = board(pool, &admin, p, &[]).await;
    let id = k.full.board.id;
    assert!(matches!(
        lanes::create_lane(pool, &admin, Origin::Web, id, sprint(&k), "Login").await,
        Err(Error::Refused(_))
    ));
    let kind = field(pool, "Type").await;
    let epic = value(pool, p, kind, "Epic").await;
    set_lanes(
        pool,
        &admin,
        id,
        LanesChange {
            field: Some(kind),
            values: vec![format!("v:{epic}")],
            ..by_parents()
        },
    )
    .await
    .unwrap();
    let first = lanes::create_lane(pool, &admin, Origin::Web, id, sprint(&k), "Login")
        .await
        .unwrap();
    let (second, described) =
        lanes::create_lane(pool, &admin, Origin::Web, id, sprint(&k), "Search")
            .await
            .unwrap();
    let made = second.issue.unwrap();
    assert_eq!(described.issues[0].issue.summary, "Search");
    assert_eq!(held(pool, made, kind).await, vec![epic]);
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec![first.0.key, second.key, "other".to_owned()]);
    assert!(v.cards.is_empty());
    assert!(matches!(
        lanes::create_lane(pool, &admin, Origin::Web, id, sprint(&k), " ").await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn brd_056_lane_to_backlog() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let story = titled(pool, &admin, p, "Story").await;
    let a = titled(pool, &admin, p, "A").await;
    let c = titled(pool, &admin, p, "C").await;
    subtask(pool, &admin, story, a).await;
    let k = board(pool, &admin, p, &[c]).await;
    let id = k.full.board.id;
    set_lanes(pool, &admin, id, by_parents()).await.unwrap();
    add(
        pool,
        &admin,
        &k,
        story,
        NewCard {
            subtasks: true,
            ..NewCard::default()
        },
    )
    .await
    .unwrap();
    let mut gone = lanes::lane_to_backlog(pool, &admin, Origin::Web, id, sprint(&k), story)
        .await
        .unwrap();
    gone.sort_unstable();
    let mut both = vec![story, a];
    both.sort_unstable();
    assert_eq!(gone, both);
    let v = show(pool, &admin, id, None).await;
    assert_eq!(keys(&v), vec!["other".to_owned()]);
    assert_eq!(v.cards.iter().map(|c| c.issue).collect::<Vec<_>>(), vec![c]);
}

#[tokio::test]
async fn brd_057_lane_pages() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR").await;
    let a = titled(pool, &admin, p, "A").await;
    let k = board(pool, &admin, p, &[a]).await;
    let id = k.full.board.id;
    let assignee = field(pool, "Assignee").await;
    // 55 account lanes (unknown accounts hold no cards) and `other` at the bottom.
    let values: Vec<String> = (1_000..1_055).map(|u| format!("u:{u}")).collect();
    set_lanes(pool, &admin, id, by_values(Some(assignee), values))
        .await
        .unwrap();
    let first = show(pool, &admin, id, None).await;
    assert_eq!(first.lanes.len(), 50);
    assert_eq!(first.lanes_total, 56);
    assert!(
        first.cards.is_empty(),
        "the card is in `other`, on page two"
    );
    assert_eq!(first.total, 1);
    let next = first.lanes_next.clone().unwrap();
    let second = show(pool, &admin, id, Some(&next)).await;
    assert_eq!(second.lanes.len(), 6);
    assert_eq!(second.lanes_next, None);
    assert_eq!(lane_of(&second, a), Some("other".to_owned()));
    assert!(matches!(
        view::view(pool, &admin, id, None, None, Some("x"), None).await,
        Err(Error::Invalid(_))
    ));
    // A cell of a board with lanes is asked for with its lane.
    let column = second.cards[0].column;
    assert!(matches!(
        view::cell(pool, &admin, id, None, column, None, None, None, None).await,
        Err(Error::Invalid(_))
    ));
    let cell = view::cell(
        pool,
        &admin,
        id,
        None,
        column,
        Some("other"),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(cell.cards.len(), 1);
}
