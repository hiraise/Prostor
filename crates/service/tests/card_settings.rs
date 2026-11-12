//! Card settings, health and field usage (BRD-022, BRD-023, BRD-028, BRD-059…BRD-062, BRD-077,
//! FLD-062; plan step 1.8 C2) on a real database.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use time::OffsetDateTime;
use time::macros::datetime;
use trackflow_domain::access::Access;
use trackflow_domain::boards::health::{Finding, Slot};
use trackflow_domain::boards::lanes::{LaneMode, OtherLane};
use trackflow_domain::boards::{ColorMode, Mode, Preset};
use trackflow_domain::issues::history::Origin;
use trackflow_domain::issues::values::Input;
use trackflow_domain::projects::Template;
use trackflow_domain::settings::Language;
use trackflow_domain::sharing::Audience;
use trackflow_domain::validation::Problem;
use trackflow_domain::{BoardId, FieldId, IssueId, ProjectId};
use trackflow_service::boards::card_setup::CardsChange;
use trackflow_service::boards::drafts::{self, Cell};
use trackflow_service::boards::lanes::LanesChange;
use trackflow_service::boards::{
    self as boards, BoardChange, BoardRow, NewBoard, Sharing, columns, health, view,
};
use trackflow_service::issues::{self, FieldChange, NewIssue};
use trackflow_service::projects::{self, NewProjectInput};
use trackflow_service::setup::{self, Setup};
use trackflow_service::users::{self, NewAccount};
use trackflow_service::{Error, Passwords, access, fields, roles};
use trackflow_store::PgPool;
use trackflow_store::access::Holder;
use trackflow_store::sessions::Origin as SessionOrigin;
use trackflow_testkit::TestDb;

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

/// An account that reads and updates issues of the project, private fields left out.
async fn updater(pool: &PgPool, admin: &Access, username: &str, of: ProjectId) -> Access {
    let user = users::create(
        pool,
        &Passwords::new(4),
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
    let role = roles::create(
        pool,
        admin,
        roles::NewRole {
            name: format!("Updater {username}"),
            description: String::new(),
            permissions: [
                "project.basic",
                "issue.read",
                "issue.update",
                "issue.create",
            ]
            .map(str::to_owned)
            .to_vec(),
        },
    )
    .await
    .unwrap();
    roles::assign(pool, admin, role.role.id, Holder::User(user), &[Some(of)])
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

async fn board(pool: &PgPool, who: &Access, preset: Preset, p: ProjectId) -> BoardRow {
    boards::create(
        pool,
        who,
        Language::En,
        NewBoard {
            preset,
            name: "Team".to_owned(),
            projects: vec![p],
            sharing: Sharing::default(),
        },
    )
    .await
    .unwrap()
}

async fn field(pool: &PgPool, name: &str) -> FieldId {
    sqlx::query_scalar("select id from fields where name = $1 order by id limit 1")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A set value by name (the scrum type set holds one «User story»).
async fn value_id(pool: &PgPool, name: &str) -> i64 {
    sqlx::query_scalar("select id from field_values where name = $1 order by id limit 1")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
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

async fn change(
    pool: &PgPool,
    who: &Access,
    id: BoardId,
    c: BoardChange,
) -> Result<BoardRow, Error> {
    let now = boards::get(pool, who, id).await.unwrap();
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

fn cards_of(row: &BoardRow) -> CardsChange {
    let b = &row.full.board;
    CardsChange {
        fields: b.card_fields.clone(),
        initials: b.card_initials.clone(),
        color: ColorMode::parse(&b.color_mode).unwrap(),
        color_field: b.color_field_id,
        estimation: b.estimation_field_id,
        original_estimation: b.original_estimation_field_id,
        project_colors: Vec::new(),
    }
}

async fn set_cards(
    pool: &PgPool,
    who: &Access,
    id: BoardId,
    c: CardsChange,
) -> Result<BoardRow, Error> {
    change(
        pool,
        who,
        id,
        BoardChange {
            cards: Some(c),
            ..BoardChange::default()
        },
    )
    .await
}

fn invalid_at(e: Error, path: &str) -> Problem {
    match e {
        Error::Invalid(problems) => problems
            .iter()
            .find(|p| p.path == path)
            .map(|p| p.problem)
            .unwrap_or_else(|| panic!("no problem at {path}: {problems:?}")),
        other => panic!("expected invalid at {path}, got {other:?}"),
    }
}

#[tokio::test]
async fn brd_059_default_fields_and_changes() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let k = board(pool, &admin, Preset::Scrum, p).await;
    let b = &k.full.board;
    let assignee = field(pool, "Assignee").await;
    // Three fields of the project, none the board uses otherwise.
    assert_eq!(b.card_fields.len(), 3);
    for taken in [
        b.column_field_id,
        b.color_field_id,
        b.estimation_field_id,
        Some(assignee),
    ] {
        assert!(!b.card_fields.contains(&taken.unwrap()), "{taken:?}");
    }
    let type_field = field(pool, "Type").await;
    let priority = field(pool, "Priority").await;
    let changed = set_cards(
        pool,
        &admin,
        b.id,
        CardsChange {
            fields: vec![priority, type_field, priority],
            initials: vec![type_field],
            ..cards_of(&k)
        },
    )
    .await
    .unwrap();
    assert_eq!(changed.full.board.card_fields, vec![priority, type_field]);
    assert_eq!(changed.full.board.card_initials, vec![type_field]);
    let stray = set_cards(
        pool,
        &admin,
        b.id,
        CardsChange {
            fields: vec![priority],
            initials: vec![type_field],
            ..cards_of(&changed)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_at(stray, "cards.initials"), Problem::Invalid);
    let other = project(pool, &admin, "OTH", Template::Kanban).await;
    let kanban_only: FieldId = sqlx::query_scalar(
        "select field_id from project_fields where project_id = $1
         except select field_id from project_fields where project_id = $2 limit 1",
    )
    .bind(other)
    .bind(p)
    .fetch_one(pool)
    .await
    .unwrap();
    let outside = set_cards(
        pool,
        &admin,
        b.id,
        CardsChange {
            fields: vec![kanban_only],
            initials: vec![],
            ..cards_of(&changed)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_at(outside, "cards.fields"), Problem::Invalid);
    let many: Vec<FieldId> = (1..=11).collect();
    let too_many = set_cards(
        pool,
        &admin,
        b.id,
        CardsChange {
            fields: many,
            initials: vec![],
            ..cards_of(&changed)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_at(too_many, "cards.fields"), Problem::TooLong);
}

#[tokio::test]
async fn brd_060_colors() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let k = board(pool, &admin, Preset::Kanban, p).await;
    let id = k.full.board.id;
    let points = field(pool, "Story points").await;
    let priority = field(pool, "Priority").await;
    let missing = set_cards(
        pool,
        &admin,
        id,
        CardsChange {
            color: ColorMode::Field,
            color_field: None,
            ..cards_of(&k)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_at(missing, "cards.color_field"), Problem::Required);
    let numbers = set_cards(
        pool,
        &admin,
        id,
        CardsChange {
            color: ColorMode::Field,
            color_field: Some(points),
            ..cards_of(&k)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_at(numbers, "cards.color_field"), Problem::Invalid);
    let by_priority = set_cards(
        pool,
        &admin,
        id,
        CardsChange {
            color: ColorMode::Field,
            color_field: Some(priority),
            ..cards_of(&k)
        },
    )
    .await
    .unwrap();
    assert_eq!(by_priority.full.board.color_field_id, Some(priority));
    let by_project = set_cards(
        pool,
        &admin,
        id,
        CardsChange {
            color: ColorMode::Project,
            project_colors: vec![(p, Some(3))],
            ..cards_of(&by_priority)
        },
    )
    .await
    .unwrap();
    assert_eq!(by_project.full.board.color_mode, "project");
    assert_eq!(by_project.full.board.project_colors, vec![Some(3)]);
    let out_of_range = set_cards(
        pool,
        &admin,
        id,
        CardsChange {
            project_colors: vec![(p, Some(12))],
            ..cards_of(&by_project)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        invalid_at(out_of_range, "cards.project_colors"),
        Problem::OutOfRange
    );
}

#[tokio::test]
async fn brd_062_sums() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let k = board(pool, &admin, Preset::Kanban, p).await;
    let id = k.full.board.id;
    let points = field(pool, "Story points").await;
    let words = set_cards(
        pool,
        &admin,
        id,
        CardsChange {
            estimation: Some(field(pool, "Type").await),
            ..cards_of(&k)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_at(words, "cards.estimation"), Problem::Invalid);
    set_cards(
        pool,
        &admin,
        id,
        CardsChange {
            estimation: Some(points),
            ..cards_of(&k)
        },
    )
    .await
    .unwrap();
    let sprint = k.full.sprints[0].id;
    let story = value_id(pool, "User story").await;
    for (n, est) in [(1, Some(3)), (2, Some(5)), (3, None)] {
        let issue = titled(pool, &admin, p, &format!("Issue {n}")).await;
        // A story, not the template's default Task: the scrum rule keeps story points from
        // tasks (AUT-085).
        issues::set_field(
            pool,
            &admin,
            Origin::Web,
            issue,
            field(pool, "Type").await,
            FieldChange::Set(Some(Input::Integer(story))),
            false,
        )
        .await
        .unwrap();
        if let Some(est) = est {
            issues::set_field(
                pool,
                &admin,
                Origin::Web,
                issue,
                points,
                FieldChange::Set(Some(Input::Integer(est))),
                false,
            )
            .await
            .unwrap();
        }
        boards::cards::add(
            pool,
            &admin,
            Origin::Web,
            id,
            sprint,
            boards::cards::NewCard {
                issue,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
    let v = view::view(pool, &admin, id, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(v.estimate, Some(8.0));
    let first = v.columns.iter().find(|c| c.count > 0).unwrap();
    assert_eq!(first.estimate, Some(8.0));
    assert!(v.columns.iter().all(|c| c.estimate.is_some()));
    // Lanes by type: each lane sums its own cards.
    let type_field = field(pool, "Type").await;
    change(
        pool,
        &admin,
        id,
        BoardChange {
            lanes: Some(LanesChange {
                mode: LaneMode::Values,
                field: Some(type_field),
                attribute: None,
                values: Vec::new(),
                other: OtherLane::Bottom,
                new_card_value: None,
            }),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let v = view::view(pool, &admin, id, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(v.estimate, Some(8.0));
    let lanes: f64 = v.lanes.iter().filter_map(|l| l.estimate).sum();
    assert_eq!(lanes, 8.0);
    // A board without an estimation sums nothing.
    let plain = board(pool, &admin, Preset::Kanban, p).await;
    let v = view::view(pool, &admin, plain.full.board.id, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(v.estimate, None);
}

#[tokio::test]
async fn brd_023_health() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let k = board(pool, &admin, Preset::Kanban, p).await;
    let id = k.full.board.id;
    assert!(k.health.is_empty(), "{:?}", k.health);
    // A setting naming a field of a wrong kind (left by a type change, FLD-066) warns.
    sqlx::query("update boards set estimation_field_id = $2 where id = $1")
        .bind(id)
        .bind(field(pool, "Type").await)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("update projects set archived_at = now() where id = $1")
        .bind(p)
        .execute(pool)
        .await
        .unwrap();
    let k = boards::get(pool, &admin, id).await.unwrap();
    assert_eq!(
        k.health,
        vec![
            Finding::ArchivedProject { project: p },
            Finding::FieldWrongKind {
                slot: Slot::Estimation
            },
        ]
    );
    sqlx::query("update projects set archived_at = null where id = $1")
        .bind(p)
        .execute(pool)
        .await
        .unwrap();
    let k = change(
        pool,
        &admin,
        id,
        BoardChange {
            column_field: Some(None),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(k.health[0], Finding::NoColumnField);
    let state = field(pool, "State").await;
    let k = change(
        pool,
        &admin,
        id,
        BoardChange {
            column_field: Some(Some(state)),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(k.health[0], Finding::NoColumns);
    // A second project without the column field cannot show its issues.
    let other = project(pool, &admin, "OTH", Template::Kanban).await;
    sqlx::query("delete from project_fields where project_id = $1 and field_id = $2")
        .bind(other)
        .bind(state)
        .execute(pool)
        .await
        .unwrap();
    let k = change(
        pool,
        &admin,
        id,
        BoardChange {
            projects: Some(vec![p, other]),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert!(
        k.health
            .contains(&Finding::ColumnFieldMissing { project: other })
    );
}

#[tokio::test]
async fn brd_028_query_mode_needs_default_columns() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let k = board(pool, &admin, Preset::Kanban, p).await;
    let id = k.full.board.id;
    let state = k.full.board.column_field_id.unwrap();
    let default: i64 = sqlx::query_scalar(
        "select default_values[1] from project_fields where project_id = $1 and field_id = $2",
    )
    .bind(p)
    .bind(state)
    .fetch_one(pool)
    .await
    .unwrap();
    let column = k
        .full
        .columns
        .iter()
        .find(|c| c.value_id == default)
        .unwrap()
        .id;
    let query = change(
        pool,
        &admin,
        id,
        BoardChange {
            mode: Some(Mode::Query),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert!(query.health.is_empty(), "{:?}", query.health);
    // On a query board the default's column stays.
    let refused = columns::delete(pool, &admin, id, column).await.unwrap_err();
    assert_eq!(invalid_at(refused, "column"), Problem::Invalid);
    change(
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
    columns::delete(pool, &admin, id, column).await.unwrap();
    let back = change(
        pool,
        &admin,
        id,
        BoardChange {
            mode: Some(Mode::Query),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_at(back, "mode"), Problem::Invalid);
}

#[tokio::test]
async fn brd_022_usage() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let k = board(pool, &admin, Preset::Scrum, p).await;
    let estimation = k.full.board.estimation_field_id.unwrap();
    let (found, others) = health::field_usage(pool, &admin, estimation).await.unwrap();
    assert_eq!(others, 0);
    // The project's own template board and this one.
    assert_eq!(found.len(), 2);
    let ours = found
        .iter()
        .find(|u| u.board.id == k.full.board.id)
        .unwrap();
    assert!(ours.estimation && !ours.column && !ours.cards);
    let column = k.full.board.column_field_id.unwrap();
    let (found, _) = health::field_usage(pool, &admin, column).await.unwrap();
    assert!(found.iter().all(|u| u.column));
    let (_, _, boards) = fields::deletion_impact(pool, &admin, estimation)
        .await
        .unwrap();
    assert_eq!(boards, 2);
}

#[tokio::test]
async fn fld_062_board_settings_hide_private() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let k = board(pool, &admin, Preset::Kanban, p).await;
    let id = k.full.board.id;
    let priority = field(pool, "Priority").await;
    let type_field = field(pool, "Type").await;
    let points = field(pool, "Story points").await;
    set_cards(
        pool,
        &admin,
        id,
        CardsChange {
            fields: vec![type_field, priority],
            initials: vec![priority],
            estimation: Some(points),
            ..cards_of(&k)
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "update project_fields set private = true
         where project_id = $1 and field_id = any($2)",
    )
    .bind(p)
    .bind(vec![priority, points])
    .execute(pool)
    .await
    .unwrap();
    let eve = updater(pool, &admin, "eve", p).await;
    change(
        pool,
        &admin,
        id,
        BoardChange {
            sharing: Some(Sharing {
                edit: Audience {
                    users: vec![eve.user()],
                    groups: Vec::new(),
                },
                ..Sharing::default()
            }),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let seen = boards::get(pool, &eve, id).await.unwrap();
    assert!(!seen.shown(priority) && !seen.shown(points) && seen.shown(type_field));
    assert!(boards::get(pool, &admin, id).await.unwrap().shown(priority));
    // Eve sees no estimation: the view sums nothing for her.
    let v = view::view(pool, &eve, id, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(v.estimate, None);
    // Her change keeps what she could not see, and she cannot pick it.
    let kept = set_cards(
        pool,
        &eve,
        id,
        CardsChange {
            fields: vec![],
            initials: vec![],
            color: ColorMode::None,
            color_field: None,
            estimation: None,
            original_estimation: None,
            project_colors: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert_eq!(kept.full.board.card_fields, vec![priority]);
    assert_eq!(kept.full.board.card_initials, vec![priority]);
    assert_eq!(kept.full.board.estimation_field_id, Some(points));
    let picked = set_cards(
        pool,
        &eve,
        id,
        CardsChange {
            fields: vec![],
            initials: vec![],
            color: ColorMode::None,
            color_field: None,
            estimation: None,
            original_estimation: Some(points),
            project_colors: Vec::new(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        invalid_at(picked, "cards.original_estimation"),
        Problem::Invalid
    );
}

#[tokio::test]
async fn brd_077_cell_draft_kept() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "SCR", Template::Scrum).await;
    let k = board(pool, &admin, Preset::Kanban, p).await;
    let id = k.full.board.id;
    let column = k.full.columns[1].clone();
    let cell = Cell {
        sprint: k.full.sprints[0].id,
        column: column.id,
        lane: None,
        project: None,
    };
    let (draft, created) = drafts::cell_draft(pool, &admin, Origin::Web, id, cell.clone())
        .await
        .unwrap();
    assert!(created);
    let state = k.full.board.column_field_id.unwrap();
    let held: Vec<i64> = sqlx::query_scalar(
        "select value from issue_ref_values where issue_id = $1 and field_id = $2",
    )
    .bind(draft)
    .bind(state)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(held, vec![column.value_id]);
    let again = drafts::cell_draft(pool, &admin, Origin::Web, id, cell.clone())
        .await
        .unwrap();
    assert_eq!(again, (draft, false));
    // Another cell has its own draft.
    let next = Cell {
        column: k.full.columns[0].id,
        ..cell.clone()
    };
    let (other, _) = drafts::cell_draft(pool, &admin, Origin::Web, id, next)
        .await
        .unwrap();
    assert_ne!(other, draft);
    // Published, the draft is a card of the cell, and the cell gets a new draft.
    sqlx::query("update issues set summary = 'From the cell' where id = $1")
        .bind(draft)
        .execute(pool)
        .await
        .unwrap();
    issues::publish(pool, &admin, Origin::Web, draft)
        .await
        .unwrap();
    let v = view::view(pool, &admin, id, None, None, None, None)
        .await
        .unwrap();
    assert!(
        v.cards
            .iter()
            .any(|c| c.issue == draft && c.column == column.id)
    );
    let (fresh, created) = drafts::cell_draft(pool, &admin, Origin::Web, id, cell)
        .await
        .unwrap();
    assert!(created && fresh != draft);
}
