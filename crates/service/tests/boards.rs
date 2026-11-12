//! Boards (BRD-003…BRD-016, BRD-042…BRD-046, BRD-057, BRD-068; plan step 1.8 A2) on a real
//! database. Cards are put on sprints with SQL until moves come (1.8a-3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use time::OffsetDateTime;
use time::macros::datetime;
use trackflow_domain::access::Access;
use trackflow_domain::access::role::PredefinedRole;
use trackflow_domain::boards::Preset;
use trackflow_domain::boards::columns::WipState;
use trackflow_domain::issues::history::Origin;
use trackflow_domain::projects::Template;
use trackflow_domain::settings::Language;
use trackflow_domain::sharing::Audience;
use trackflow_domain::validation::Problem;
use trackflow_domain::{BoardId, GroupId, IssueId, ProjectId, SprintId, UserId};
use trackflow_service::boards::cards;
use trackflow_service::boards::columns::{self, ColumnChange, NewColumn};
use trackflow_service::boards::view::{self, CELL_CAP};
use trackflow_service::boards::{
    self as boards, BoardChange, BoardRow, NewBoard, ProfileChange, Sharing,
};
use trackflow_service::groups::NewGroup;
use trackflow_service::issues::{self, AudienceInput, NewIssue};
use trackflow_service::projects::{self, NewProjectInput};
use trackflow_service::setup::{self, Setup};
use trackflow_service::users::{self, NewAccount};
use trackflow_service::{Error, Passwords, access, groups, roles};
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

async fn group(pool: &PgPool, admin: &Access, name: &str, members: &[UserId]) -> GroupId {
    let id = groups::create(
        pool,
        admin,
        NewGroup {
            name: name.to_owned(),
            description: String::new(),
            auto_join: false,
        },
    )
    .await
    .unwrap()
    .id;
    if !members.is_empty() {
        groups::add_members(pool, admin, id, members).await.unwrap();
    }
    id
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

fn users_(ids: &[UserId]) -> Audience {
    Audience {
        users: ids.to_vec(),
        groups: Vec::new(),
    }
}

async fn create(
    pool: &PgPool,
    who: &Access,
    preset: Preset,
    projects: &[ProjectId],
    sharing: Sharing,
) -> Result<BoardRow, Error> {
    boards::create(
        pool,
        who,
        Language::En,
        NewBoard {
            preset,
            name: "Team".to_owned(),
            projects: projects.to_vec(),
            sharing,
        },
    )
    .await
}

async fn change(
    pool: &PgPool,
    who: &Access,
    b: &BoardRow,
    c: BoardChange,
) -> Result<BoardRow, Error> {
    boards::update(
        pool,
        who,
        Language::En,
        b.full.board.id,
        BoardChange {
            version: b.full.board.version,
            ..c
        },
    )
    .await
}

/// The sprint the board shows.
async fn shown_sprint(pool: &PgPool, who: &Access, board: BoardId) -> SprintId {
    view::view(pool, who, board, None, None, None, None)
        .await
        .unwrap()
        .sprint
        .unwrap()
        .id
}

/// Puts issues on a sprint with ranks in the given order (moves are 1.8a-3).
async fn put(pool: &PgPool, sprint: SprintId, issues: &[IssueId]) {
    let ranks = trackflow_domain::rank::sequence(issues.len());
    sqlx::query(
        "insert into sprint_issues (sprint_id, issue_id, rank)
         select $1, i, r from unnest($2::bigint[], $3::text[]) as t(i, r)",
    )
    .bind(sprint)
    .bind(issues)
    .bind(&ranks)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn brd_003_create_needs_read() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let a = project(pool, &admin, "AA", Template::Kanban).await;
    let b = project(pool, &admin, "BB", Template::Kanban).await;
    let mia = member(pool, &admin, "mia", &[a]).await;
    assert!(
        create(pool, &mia, Preset::Kanban, &[a], Sharing::default())
            .await
            .is_ok()
    );
    assert!(matches!(
        create(pool, &mia, Preset::Kanban, &[a, b], Sharing::default()).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        create(pool, &mia, Preset::Kanban, &[], Sharing::default()).await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn brd_003_presets() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Scrum).await;

    let scrum = create(pool, &admin, Preset::Scrum, &[p], Sharing::default())
        .await
        .unwrap();
    assert!(scrum.full.board.sprints_on);
    assert!(scrum.full.board.column_field_id.is_some());
    assert!(scrum.full.columns.len() >= 2);
    let visible: Vec<_> = scrum.full.sprints.iter().filter(|s| !s.internal).collect();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "Sprint 1");
    assert!(scrum.full.columns.iter().all(|c| c.wip_max.is_none()));
    // The last column is the first resolved value.
    assert!(scrum.column_values.last().unwrap().resolved);
    assert!(
        scrum.column_values[..scrum.column_values.len() - 1]
            .iter()
            .all(|v| !v.resolved)
    );

    let kanban = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    assert!(!kanban.full.board.sprints_on);
    assert_eq!(kanban.full.sprints.len(), 1);
    assert!(kanban.full.sprints[0].internal);
    let n = kanban.full.columns.len();
    assert!(n >= 3);
    assert_eq!(kanban.full.columns[0].wip_max, None);
    assert_eq!(kanban.full.columns[1].wip_max, Some(5));
    assert_eq!(kanban.full.columns[n - 1].wip_max, None);

    let custom = create(pool, &admin, Preset::Custom, &[p], Sharing::default())
        .await
        .unwrap();
    assert!(custom.full.board.sprints_on);
    assert_eq!(custom.full.board.column_field_id, None);
    assert!(custom.full.columns.is_empty());
    assert!(custom.rights.edit && custom.rights.delete);
}

#[tokio::test]
async fn brd_010_projects() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let a = project(pool, &admin, "AA", Template::Kanban).await;
    let b = project(pool, &admin, "BB", Template::Kanban).await;
    let c = project(pool, &admin, "CC", Template::Kanban).await;
    let mia = member(pool, &admin, "mia", &[a, b]).await;
    let board = create(pool, &mia, Preset::Kanban, &[a], Sharing::default())
        .await
        .unwrap();
    let not_read = change(
        pool,
        &mia,
        &board,
        BoardChange {
            projects: Some(vec![a, c]),
            ..BoardChange::default()
        },
    )
    .await;
    assert!(matches!(not_read, Err(Error::Invalid(_))));
    let both = change(
        pool,
        &mia,
        &board,
        BoardChange {
            projects: Some(vec![b, a]),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(both.full.board.projects, vec![a, b]);
    assert!(matches!(
        change(pool, &mia, &board, BoardChange::default()).await,
        Err(Error::Conflict(_))
    ));
    let listed = boards::list(pool, &mia, None, Some(b), None, 10)
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    let listed = boards::list(pool, &mia, Some("tea"), Some(c), None, 10)
        .await
        .unwrap();
    assert!(listed.items.is_empty());
}

#[tokio::test]
async fn brd_011_transfer_loses_access() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let mia = member(pool, &admin, "mia", &[p]).await;
    let leo = member(pool, &admin, "leo", &[p]).await;
    let board = create(pool, &mia, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let after = change(
        pool,
        &mia,
        &board,
        BoardChange {
            owner: Some(leo.user()),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(after.full.board.owner_id, leo.user());
    assert!(!after.rights.edit, "the previous owner keeps nothing");
    let id = board.full.board.id;
    assert!(matches!(
        boards::get(pool, &mia, id).await,
        Err(Error::NotFound(_))
    ));
    assert!(boards::get(pool, &leo, id).await.unwrap().rights.delete);
}

#[tokio::test]
async fn brd_012_view_readers() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let mia = member(pool, &admin, "mia", &[p]).await;
    let leo = member(pool, &admin, "leo", &[p]).await;
    let eve = member(pool, &admin, "eve", &[]).await;
    let board = create(pool, &mia, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    assert!(matches!(
        boards::get(pool, &leo, id).await,
        Err(Error::NotFound(_))
    ));
    change(
        pool,
        &mia,
        &board,
        BoardChange {
            sharing: Some(Sharing {
                view_readers: true,
                ..Sharing::default()
            }),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let seen = boards::get(pool, &leo, id).await.unwrap();
    assert!(!seen.rights.edit);
    assert!(matches!(
        boards::get(pool, &eve, id).await,
        Err(Error::NotFound(_))
    ));
    let listed = boards::list(pool, &leo, None, None, None, 10)
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert!(
        boards::list(pool, &eve, None, None, None, 10)
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn brd_013_edit_updaters() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let mia = member(pool, &admin, "mia", &[p]).await;
    let leo = member(pool, &admin, "leo", &[p]).await;
    let eve = member(pool, &admin, "eve", &[]).await;
    let board = create(
        pool,
        &mia,
        Preset::Kanban,
        &[p],
        Sharing {
            view: users_(&[leo.user(), eve.user()]),
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    let id = board.full.board.id;
    let rename = |b: &BoardRow| BoardChange {
        name: Some("Renamed".to_owned()),
        version: b.full.board.version,
        ..BoardChange::default()
    };
    assert!(matches!(
        boards::update(pool, &leo, Language::En, id, rename(&board)).await,
        Err(Error::Refused(_))
    ));
    let board = change(
        pool,
        &mia,
        &board,
        BoardChange {
            sharing: Some(Sharing {
                view: users_(&[leo.user(), eve.user()]),
                edit_updaters: true,
                ..Sharing::default()
            }),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert!(board.rights.share);
    let renamed = boards::update(pool, &leo, Language::En, id, rename(&board))
        .await
        .unwrap();
    assert_eq!(renamed.full.board.name, "Renamed");
    // Eve views the board but updates no issue of its project.
    assert!(matches!(
        boards::update(pool, &eve, Language::En, id, rename(&renamed)).await,
        Err(Error::Refused(_))
    ));
    // `system.write` reaches only boards the administrator views.
    assert!(matches!(
        boards::get(pool, &admin, id).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn brd_016_delete_impact() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Scrum).await;
    let mia = member(pool, &admin, "mia", &[p]).await;
    let leo = member(pool, &admin, "leo", &[p]).await;
    let board = create(
        pool,
        &mia,
        Preset::Scrum,
        &[p],
        Sharing {
            edit: users_(&[leo.user()]),
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &mia, id).await;
    let one = issue(pool, &mia, p).await;
    let two = issue(pool, &mia, p).await;
    put(pool, sprint, &[one, two]).await;
    let impact = boards::impact(pool, &leo, id).await.unwrap();
    assert_eq!((impact.sprints, impact.cards, impact.searches), (1, 2, 0));
    assert!(matches!(
        boards::delete(pool, &leo, id).await,
        Err(Error::Refused(_))
    ));
    // An administrator deletes even a board they do not see.
    let private = create(pool, &mia, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    boards::delete(pool, &admin, private.full.board.id)
        .await
        .unwrap();
    boards::delete(pool, &mia, id).await.unwrap();
    assert!(matches!(
        boards::get(pool, &mia, id).await,
        Err(Error::NotFound(_))
    ));
    // The issues stay.
    assert_eq!(
        issues::describe_many(pool, &mia, &[one, two])
            .await
            .unwrap()
            .issues
            .len(),
        2
    );
}

/// A custom board over a scrum project with its state field as the column field.
async fn custom_with_field(pool: &PgPool, who: &Access, p: ProjectId) -> BoardRow {
    let scrum = create(pool, who, Preset::Scrum, &[p], Sharing::default())
        .await
        .unwrap();
    let field = scrum.full.board.column_field_id.unwrap();
    let custom = create(pool, who, Preset::Custom, &[p], Sharing::default())
        .await
        .unwrap();
    change(
        pool,
        who,
        &custom,
        BoardChange {
            column_field: Some(Some(field)),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn brd_043_add_move_remove_columns() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Scrum).await;
    let scrum = create(pool, &admin, Preset::Scrum, &[p], Sharing::default())
        .await
        .unwrap();
    let values: Vec<i64> = scrum.column_values.iter().map(|v| v.id).collect();
    let board = custom_with_field(pool, &admin, p).await;
    let id = board.full.board.id;
    assert!(board.full.columns.is_empty());

    let one = columns::add(pool, &admin, id, NewColumn::Value(values[0]), None)
        .await
        .unwrap();
    let two = columns::add(pool, &admin, id, NewColumn::Value(values[1]), None)
        .await
        .unwrap();
    let order: Vec<i64> = two.full.columns.iter().map(|c| c.value_id).collect();
    assert_eq!(order, vec![values[0], values[1]]);
    let again = columns::add(pool, &admin, id, NewColumn::Value(values[0]), None).await;
    assert!(
        matches!(&again, Err(Error::Invalid(p)) if p.iter().any(|x| x.problem == Problem::Taken))
    );
    assert!(matches!(
        columns::add(pool, &admin, id, NewColumn::Value(999_999), None).await,
        Err(Error::Invalid(_))
    ));

    // Move the second first; WIP limits are checked.
    let second = two.full.columns[1].id;
    let moved = columns::update(
        pool,
        &admin,
        id,
        second,
        ColumnChange {
            position: Some(None),
            wip_max: Some(Some(3)),
            ..ColumnChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(moved.full.columns[0].id, second);
    assert_eq!(moved.full.columns[0].wip_max, Some(3));
    assert!(matches!(
        columns::update(
            pool,
            &admin,
            id,
            second,
            ColumnChange {
                wip_min: Some(Some(4)),
                ..ColumnChange::default()
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
    let first = one.full.columns[0].id;
    let left = columns::delete(pool, &admin, id, first).await.unwrap();
    assert_eq!(left.full.columns.len(), 1);
    assert!(matches!(
        columns::delete(pool, &admin, id, first).await,
        Err(Error::NotFound(_))
    ));
    // A new column field drops the columns.
    let dropped = change(
        pool,
        &admin,
        &left,
        BoardChange {
            column_field: Some(None),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    assert!(dropped.full.columns.is_empty());
}

#[tokio::test]
async fn brd_043_column_field_rules() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Scrum).await;
    let custom = create(pool, &admin, Preset::Custom, &[p], Sharing::default())
        .await
        .unwrap();
    assert!(matches!(
        columns::add(
            pool,
            &admin,
            custom.full.board.id,
            NewColumn::Value(1),
            None
        )
        .await,
        Err(Error::Conflict(_))
    ));
    let summary_like = sqlx::query_scalar::<_, i64>(
        "select f.id from fields f join project_fields pf on pf.field_id = f.id
         where pf.project_id = $1 and (f.multi or f.kind in ('user', 'date', 'integer'))
         limit 1",
    )
    .bind(p)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(matches!(
        change(
            pool,
            &admin,
            &custom,
            BoardChange {
                column_field: Some(Some(summary_like)),
                ..BoardChange::default()
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn brd_044_new_value_and_rename_need_set_rights() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Scrum).await;
    let mia = member(pool, &admin, "mia", &[p]).await;
    let board = create(pool, &mia, Preset::Scrum, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    assert!(matches!(
        columns::add(
            pool,
            &mia,
            id,
            NewColumn::NewValue("Review".to_owned()),
            None
        )
        .await,
        Err(Error::Refused(_))
    ));
    let column = board.full.columns[0].id;
    assert!(matches!(
        columns::update(
            pool,
            &mia,
            id,
            column,
            ColumnChange {
                name: Some("Todo".to_owned()),
                ..ColumnChange::default()
            },
        )
        .await,
        Err(Error::Refused(_))
    ));

    // The administrator edits the set; the board is shared with them to edit it.
    let board = change(
        pool,
        &mia,
        &board,
        BoardChange {
            sharing: Some(Sharing {
                edit: users_(&[admin.user()]),
                ..Sharing::default()
            }),
            ..BoardChange::default()
        },
    )
    .await
    .unwrap();
    let n = board.full.columns.len();
    let added = columns::add(
        pool,
        &admin,
        id,
        NewColumn::NewValue("Review".to_owned()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(added.full.columns.len(), n + 1);
    assert_eq!(added.column_values.last().unwrap().name, "Review");
    let renamed = columns::update(
        pool,
        &admin,
        id,
        column,
        ColumnChange {
            name: Some("Todo".to_owned()),
            ..ColumnChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.column_values[0].name, "Todo");
}

/// The column of an issue's state on this board.
async fn column_of(pool: &PgPool, board: &BoardRow, issue: IssueId) -> i64 {
    let value: i64 = sqlx::query_scalar(
        "select value from issue_ref_values where issue_id = $1 and field_id = $2",
    )
    .bind(issue)
    .bind(board.full.board.column_field_id.unwrap())
    .fetch_one(pool)
    .await
    .unwrap();
    board
        .full
        .columns
        .iter()
        .find(|c| c.value_id == value)
        .unwrap()
        .id
}

#[tokio::test]
async fn brd_057_cell_cap() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let board = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let many = usize::try_from(CELL_CAP).unwrap() + 3;
    let mut ids = Vec::with_capacity(many);
    for _ in 0..many {
        ids.push(issue(pool, &admin, p).await);
    }
    put(pool, sprint, &ids).await;
    let column = column_of(pool, &board, ids[0]).await;

    let v = view::view(pool, &admin, id, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(v.cards.len(), usize::try_from(CELL_CAP).unwrap());
    assert_eq!(v.cards[0].issue, ids[0], "in rank order");
    assert_eq!(v.issues.issues.len(), v.cards.len());
    assert_eq!(v.total, i64::try_from(many).unwrap());
    assert_eq!(v.cells.len(), 1);
    assert_eq!(v.cells[0].column, column);
    assert_eq!(v.cells[0].total, i64::try_from(many).unwrap());
    let header = v.columns.iter().find(|c| c.column == column).unwrap();
    assert_eq!(header.count, i64::try_from(many).unwrap());
    // Kanban: WIP max 5 on the middle columns; the first column has none.
    assert!(matches!(header.wip, WipState::Within | WipState::Over));

    // The first page of the cell is the board's; the cursor gives the rest.
    let first = view::cell(pool, &admin, id, None, column, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(first.cards.len(), usize::try_from(CELL_CAP).unwrap());
    let rest = view::cell(
        pool,
        &admin,
        id,
        None,
        column,
        None,
        first.next_cursor.as_deref(),
        None,
        None,
    )
    .await
    .unwrap();
    let rest_ids: Vec<IssueId> = rest.cards.iter().map(|c| c.issue).collect();
    assert_eq!(rest_ids, ids[usize::try_from(CELL_CAP).unwrap()..].to_vec());
    assert_eq!(rest.next_cursor, None);
    // The view's truncated cell continues where the view stopped.
    assert_eq!(v.cells[0].next_cursor, first.next_cursor);
    assert!(matches!(
        view::cell(
            pool,
            &admin,
            id,
            None,
            column,
            None,
            Some("junk"),
            None,
            None
        )
        .await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn brd_068_view_visibility() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let mia = member(pool, &admin, "mia", &[p]).await;
    let leo = member(pool, &admin, "leo", &[p]).await;
    let board = create(
        pool,
        &mia,
        Preset::Kanban,
        &[p],
        Sharing {
            view: users_(&[leo.user()]),
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &mia, id).await;
    let open = issue(pool, &mia, p).await;
    let secret = issue_for(
        pool,
        &mia,
        p,
        Some(AudienceInput {
            users: vec![mia.user()],
            groups: Vec::new(),
        }),
    )
    .await;
    put(pool, sprint, &[open, secret]).await;
    let mine: Vec<IssueId> = view::view(pool, &mia, id, None, None, None, None)
        .await
        .unwrap()
        .cards
        .iter()
        .map(|c| c.issue)
        .collect();
    assert_eq!(mine, vec![open, secret]);
    let theirs = view::view(pool, &leo, id, None, None, None, None)
        .await
        .unwrap();
    let ids: Vec<IssueId> = theirs.cards.iter().map(|c| c.issue).collect();
    assert_eq!(ids, vec![open]);
    assert_eq!(theirs.total, 1, "counts are what the viewer sees");
    // ISS-024: cards count the files of the issue each viewer sees.
    for (key, audience) in [("a", Vec::new()), ("b", vec![mia.user()])] {
        let mut conn = pool.acquire().await.unwrap();
        let file = trackflow_store::attachments::insert(
            &mut conn,
            &trackflow_store::attachments::NewAttachment {
                issue: open,
                comment: None,
                author: mia.user(),
                name: key,
                size: 1,
                content_type: "text/plain",
                sha256: &[0; 32],
                storage_key: key,
                secret: &[0; 32],
            },
        )
        .await
        .unwrap();
        sqlx::query("update attachments set audience_users = $1 where id = $2")
            .bind(audience)
            .bind(file)
            .execute(pool)
            .await
            .unwrap();
    }
    let counted = |v: &view::BoardView| v.issues.files_of(open);
    let again = view::view(pool, &mia, id, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(counted(&again), 2);
    let theirs = view::view(pool, &leo, id, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(counted(&theirs), 1);
    // The visit is recorded, the profile is per person.
    let listed = boards::list(pool, &leo, None, None, None, 10)
        .await
        .unwrap();
    assert_eq!(listed.last_visited, Some(id));
    let profile = boards::set_profile(
        pool,
        &leo,
        id,
        ProfileChange {
            favorite: Some(true),
            card_size: Some(2),
            collapsed_columns: Some(vec![board.full.columns[0].id, 999_999]),
            ..ProfileChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(profile.collapsed_columns, vec![board.full.columns[0].id]);
    assert!(boards::get(pool, &leo, id).await.unwrap().profile.favorite);
    assert!(!boards::get(pool, &mia, id).await.unwrap().profile.favorite);
    assert!(matches!(
        boards::set_profile(
            pool,
            &leo,
            id,
            ProfileChange {
                card_size: Some(7),
                ..ProfileChange::default()
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
    // A sprint of another board is not found.
    assert!(matches!(
        view::view(pool, &leo, id, Some(sprint + 1_000), None, None, None).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn brd_deleted_account_and_group() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let mia = member(pool, &admin, "mia", &[p]).await;
    let leo = member(pool, &admin, "leo", &[p]).await;
    let eda = member(pool, &admin, "eda", &[p]).await;
    let team = group(pool, &admin, "Team", &[leo.user()]).await;
    let board = create(
        pool,
        &mia,
        Preset::Kanban,
        &[p],
        Sharing {
            view: Audience {
                users: vec![leo.user()],
                groups: vec![team],
            },
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    let id = board.full.board.id;
    boards::set_profile(
        pool,
        &leo,
        id,
        ProfileChange {
            favorite: Some(true),
            ..ProfileChange::default()
        },
    )
    .await
    .unwrap();

    // Mia leaves: the board passes to Eda.
    users::delete(pool, &admin, &[mia.user()], eda.user())
        .await
        .unwrap();
    let moved = boards::get(pool, &eda, id).await.unwrap();
    assert_eq!(moved.full.board.owner_id, eda.user());
    // Leo leaves: his place in the audience passes to the administrator, his profile goes.
    users::delete(pool, &admin, &[leo.user()], admin.user())
        .await
        .unwrap();
    let moved = boards::get(pool, &eda, id).await.unwrap();
    assert_eq!(moved.full.board.view_users, vec![admin.user()]);
    let profiles: i64 =
        sqlx::query_scalar("select count(*) from board_profiles where user_id = $1")
            .bind(leo.user())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(profiles, 0);
    // A deleted group's entries pass to the replacement group.
    let others = group(pool, &admin, "Others", &[]).await;
    groups::delete(pool, &admin, &[team], others).await.unwrap();
    let moved = boards::get(pool, &eda, id).await.unwrap();
    assert_eq!(moved.full.board.view_groups, vec![others]);
}

// Cards (1.8a-3): add, move, remove, ranks and history.

/// An account that reads the project's issues but does not update them.
async fn reader(pool: &PgPool, admin: &Access, username: &str, of: ProjectId) -> Access {
    let user = member(pool, admin, username, &[]).await.user();
    let role = roles::create(
        pool,
        admin,
        roles::NewRole {
            name: format!("Reader {username}"),
            description: String::new(),
            permissions: ["project.basic", "issue.read", "comment.read"]
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

fn new_card(issue: IssueId) -> cards::NewCard {
    cards::NewCard {
        issue,
        ..cards::NewCard::default()
    }
}

async fn version(pool: &PgPool, who: &Access, issue: IssueId) -> i64 {
    issues::get(pool, who, issue).await.unwrap().0.issue.version
}

/// The cards of the shown sprint, in board order.
async fn order(pool: &PgPool, who: &Access, board: BoardId) -> Vec<IssueId> {
    let v = view::view(pool, who, board, None, None, None, None)
        .await
        .unwrap();
    v.cards.iter().map(|c| c.issue).collect()
}

/// The board records of an issue: (removed, added).
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

#[tokio::test]
async fn brd_025_add_to_one_sprint() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Scrum).await;
    let other = project(pool, &admin, "OT", Template::Scrum).await;
    let board = create(pool, &admin, Preset::Scrum, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    let first = board.full.sprints.iter().find(|s| !s.internal).unwrap().id;
    let second: SprintId = sqlx::query_scalar(
        "insert into sprints (board_id, name) values ($1, 'Second') returning id",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    let a = issue(pool, &admin, p).await;
    let before = version(pool, &admin, a).await;

    let added = cards::add(pool, &admin, Origin::Web, id, first, new_card(a))
        .await
        .unwrap()
        .done;
    let card = added.card.unwrap();
    assert_eq!(card.issue, a);
    assert!(card.rank.is_some());
    assert_eq!(
        version(pool, &admin, a).await,
        before + 1,
        "adding is an update"
    );

    // A sprint board holds an issue in one sprint: adding it to another moves it.
    cards::add(pool, &admin, Origin::Web, id, second, new_card(a))
        .await
        .unwrap();
    let in_first: i64 =
        sqlx::query_scalar("select count(*) from sprint_issues where sprint_id = $1")
            .bind(first)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(in_first, 0);
    // Already there: nothing changes.
    let now = version(pool, &admin, a).await;
    cards::add(pool, &admin, Origin::Web, id, second, new_card(a))
        .await
        .unwrap();
    assert_eq!(version(pool, &admin, a).await, now);

    let records = board_records(pool, a).await;
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].0.as_ref().unwrap()["sprint"]["id"], first);
    assert_eq!(records[1].1.as_ref().unwrap()["sprint"]["name"], "Second");

    // Only issues of board projects, only published ones; never a sprint of another board.
    let stranger = issue(pool, &admin, other).await;
    assert!(matches!(
        cards::add(pool, &admin, Origin::Web, id, first, new_card(stranger)).await,
        Err(Error::Invalid(_))
    ));
    let draft = issues::create(
        pool,
        &admin,
        Origin::Web,
        NewIssue {
            project: p,
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
    .id;
    // A draft may join (ISS-007); its card shows once it is published.
    let joined = cards::add(pool, &admin, Origin::Web, id, first, new_card(draft))
        .await
        .unwrap()
        .done;
    assert!(joined.card.is_none());
    let elsewhere = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let foreign = shown_sprint(pool, &admin, elsewhere.full.board.id).await;
    assert!(matches!(
        cards::add(pool, &admin, Origin::Web, id, foreign, new_card(a)).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn brd_025_add_into_a_column() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let board = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let a = issue(pool, &admin, p).await;
    let target = board.full.columns[1].id;
    let added = cards::add(
        pool,
        &admin,
        Origin::Web,
        id,
        sprint,
        cards::NewCard {
            column: Some(target),
            ..new_card(a)
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(added.card.unwrap().column, target);
    assert_eq!(column_of(pool, &board, a).await, target);
    assert_eq!(added.issues.issues.len(), 1);
}

#[tokio::test]
async fn brd_068_move_sets_column_value() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let board = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let a = issue(pool, &admin, p).await;
    cards::add(pool, &admin, Origin::Web, id, sprint, new_card(a))
        .await
        .unwrap();
    let last = board.full.columns.last().unwrap().id;
    let seen = version(pool, &admin, a).await;
    let moved = cards::move_card(
        pool,
        &admin,
        Origin::Web,
        id,
        sprint,
        a,
        cards::Move {
            lane: None,
            column: last,
            after: None,
            before: None,
            version: seen,
            header: None,
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(moved.card.unwrap().column, last);
    assert_eq!(column_of(pool, &board, a).await, last);
    let fields: i64 = sqlx::query_scalar(
        "select count(*) from issue_history where issue_id = $1 and kind = 'field'",
    )
    .bind(a)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(fields, 1, "the move is a field change in history");

    // A version the caller did not see is a conflict.
    assert!(matches!(
        cards::move_card(
            pool,
            &admin,
            Origin::Web,
            id,
            sprint,
            a,
            cards::Move {
                lane: None,
                column: board.full.columns[0].id,
                after: None,
                before: None,
                version: seen,
                header: None,
            },
        )
        .await,
        Err(Error::Conflict(_))
    ));
    // Not a card of the sprint.
    let b = issue(pool, &admin, p).await;
    let vb = version(pool, &admin, b).await;
    assert!(matches!(
        cards::move_card(
            pool,
            &admin,
            Origin::Web,
            id,
            sprint,
            b,
            cards::Move {
                lane: None,
                column: last,
                after: None,
                before: None,
                version: vb,
                header: None,
            },
        )
        .await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn brd_068_refused_snaps_back() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let rita = reader(pool, &admin, "rita", p).await;
    let board = create(
        pool,
        &admin,
        Preset::Kanban,
        &[p],
        Sharing {
            view: users_(&[rita.user()]),
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    for i in [a, b] {
        cards::add(pool, &admin, Origin::Web, id, sprint, new_card(i))
            .await
            .unwrap();
    }
    let column = column_of(pool, &board, a).await;
    let seen = version(pool, &rita, a).await;
    let refused = cards::move_card(
        pool,
        &rita,
        Origin::Web,
        id,
        sprint,
        a,
        cards::Move {
            lane: None,
            column: board.full.columns.last().unwrap().id,
            after: Some(b),
            before: None,
            version: seen,
            header: None,
        },
    )
    .await;
    assert!(matches!(refused, Err(Error::Forbidden(_))), "{refused:?}");
    assert_eq!(column_of(pool, &board, a).await, column);
    assert_eq!(order(pool, &rita, id).await, vec![a, b]);
    assert_eq!(version(pool, &rita, a).await, seen);
    assert!(matches!(
        cards::add(
            pool,
            &rita,
            Origin::Web,
            id,
            sprint,
            new_card(issue(pool, &admin, p).await)
        )
        .await,
        Err(Error::Forbidden(_))
    ));
}

async fn reorder(
    pool: &PgPool,
    who: &Access,
    board: &BoardRow,
    sprint: SprintId,
    card: IssueId,
    after: Option<IssueId>,
    before: Option<IssueId>,
) {
    let column = column_of(pool, board, card).await;
    let v = version(pool, who, card).await;
    cards::move_card(
        pool,
        who,
        Origin::Web,
        board.full.board.id,
        sprint,
        card,
        cards::Move {
            lane: None,
            column,
            after,
            before,
            version: v,
            header: None,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn brd_069_after_and_first() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let board = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        let i = issue(pool, &admin, p).await;
        cards::add(pool, &admin, Origin::Web, id, sprint, new_card(i))
            .await
            .unwrap();
        ids.push(i);
    }
    let [a, b, c] = [ids[0], ids[1], ids[2]];
    assert_eq!(order(pool, &admin, id).await, vec![a, b, c], "added last");

    reorder(pool, &admin, &board, sprint, c, None, Some(a)).await;
    assert_eq!(order(pool, &admin, id).await, vec![c, a, b], "first");
    reorder(pool, &admin, &board, sprint, c, Some(a), Some(b)).await;
    assert_eq!(order(pool, &admin, id).await, vec![a, c, b], "between");
    reorder(pool, &admin, &board, sprint, a, Some(b), None).await;
    assert_eq!(
        order(pool, &admin, id).await,
        vec![c, b, a],
        "after the last"
    );
    // `before` not above `after`: the rank follows `after` alone.
    reorder(pool, &admin, &board, sprint, b, Some(a), Some(c)).await;
    assert_eq!(order(pool, &admin, id).await, vec![c, a, b]);
    // A rank change is not an update of the issue.
    let v = version(pool, &admin, b).await;
    reorder(pool, &admin, &board, sprint, b, None, Some(c)).await;
    assert_eq!(version(pool, &admin, b).await, v);
    // A neighbour must be a card of the sprint.
    let outside = issue(pool, &admin, p).await;
    let column = column_of(pool, &board, a).await;
    assert!(matches!(
        cards::move_card(
            pool,
            &admin,
            Origin::Web,
            id,
            sprint,
            a,
            cards::Move {
                lane: None,
                column,
                after: Some(outside),
                before: None,
                version: version(pool, &admin, a).await,
                header: None,
            },
        )
        .await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn brd_069_concurrent_ties() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let board = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let a = issue(pool, &admin, p).await;
    let b = issue(pool, &admin, p).await;
    let c = issue(pool, &admin, p).await;
    put(pool, sprint, &[a, c]).await;
    // Two drops at once gave b the rank of a: ties order by issue ID.
    sqlx::query(
        "insert into sprint_issues (sprint_id, issue_id, rank)
         select sprint_id, $2, rank from sprint_issues where sprint_id = $1 and issue_id = $3",
    )
    .bind(sprint)
    .bind(b)
    .bind(a)
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(order(pool, &admin, id).await, vec![a, b, c]);
    // Between two equal ranks there is no room: the rank follows `a` alone, so it lands after
    // the whole tie; the move is not refused.
    reorder(pool, &admin, &board, sprint, c, Some(a), Some(b)).await;
    assert_eq!(order(pool, &admin, id).await, vec![a, b, c]);
    reorder(pool, &admin, &board, sprint, c, None, Some(a)).await;
    assert_eq!(order(pool, &admin, id).await, vec![c, a, b]);
}

#[tokio::test]
async fn brd_074_remove_card() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let rita = reader(pool, &admin, "rita", p).await;
    let board = create(
        pool,
        &admin,
        Preset::Kanban,
        &[p],
        Sharing {
            view: users_(&[rita.user()]),
            ..Sharing::default()
        },
    )
    .await
    .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let a = issue(pool, &admin, p).await;
    cards::add(pool, &admin, Origin::Web, id, sprint, new_card(a))
        .await
        .unwrap();
    assert!(matches!(
        cards::remove(pool, &rita, Origin::Web, id, sprint, a).await,
        Err(Error::Forbidden(_))
    ));
    let v = version(pool, &admin, a).await;
    cards::remove(pool, &admin, Origin::Web, id, sprint, a)
        .await
        .unwrap();
    assert!(order(pool, &admin, id).await.is_empty());
    assert_eq!(
        version(pool, &admin, a).await,
        v + 1,
        "removing is an update"
    );
    assert!(matches!(
        cards::remove(pool, &admin, Origin::Web, id, sprint, a).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn hist_002_board_records() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let board = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let a = issue(pool, &admin, p).await;
    cards::add(pool, &admin, Origin::Web, id, sprint, new_card(a))
        .await
        .unwrap();
    cards::remove(pool, &admin, Origin::Web, id, sprint, a)
        .await
        .unwrap();
    boards::delete(pool, &admin, id).await.unwrap();
    // The records keep the board's name after it is gone; a sprintless board names no sprint.
    let records = board_records(pool, a).await;
    assert_eq!(
        records,
        vec![
            (
                None,
                Some(serde_json::json!({"board": {"id": id, "name": "Team"}}))
            ),
            (
                Some(serde_json::json!({"board": {"id": id, "name": "Team"}})),
                None
            ),
        ]
    );
}

#[tokio::test]
async fn brd_114_batch_places_cards() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let p = project(pool, &admin, "PR", Template::Kanban).await;
    let board = create(pool, &admin, Preset::Kanban, &[p], Sharing::default())
        .await
        .unwrap();
    let id = board.full.board.id;
    let sprint = shown_sprint(pool, &admin, id).await;
    let a = issue(pool, &admin, p).await;
    let off = issue(pool, &admin, p).await;
    cards::add(pool, &admin, Origin::Web, id, sprint, new_card(a))
        .await
        .unwrap();
    let (found, described) = view::batch(pool, &admin, id, None, &[off, a], None, None)
        .await
        .unwrap();
    assert_eq!(found.len(), 2);
    assert!(found[0].is_none());
    assert_eq!(found[1].as_ref().unwrap().issue, a);
    assert_eq!(described.issues.len(), 1);
    let many: Vec<IssueId> = (1..=101).collect();
    assert!(matches!(
        view::batch(pool, &admin, id, None, &many, None, None).await,
        Err(Error::Invalid(_))
    ));
}
